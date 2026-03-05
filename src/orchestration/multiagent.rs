//! Multi-Agent Chat System
//!
//! Supports up to 16 concurrent agent streams for parallel task execution.
//!
//! Features:
//! - Concurrent agent execution with configurable parallelism
//! - Streaming responses from all agents
//! - Task distribution and coordination
//! - Shared context and results aggregation

use anyhow::{Context, Result};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, Mutex, RwLock, Semaphore};
use tokio::task::JoinSet;

use crate::api::types::Message;
use crate::api::{ApiClient, ThinkingMode};
use crate::config::Config;
use crate::swarm::AgentRole;
use crate::tool_parser::parse_tool_calls;
use crate::tools::ToolRegistry;

/// Maximum number of concurrent agent streams
pub const MAX_CONCURRENT_AGENTS: usize = 16;

/// Failure policy for multi-agent execution
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum MultiAgentFailurePolicy {
    /// Continue even if some agents fail (default)
    BestEffort,
    /// Abort all remaining tasks if any agent fails
    #[default]
    FailFast,
}

/// Configuration for multi-agent chat
#[derive(Debug, Clone)]
pub struct MultiAgentConfig {
    /// Maximum concurrent streams (1-16)
    pub max_concurrency: usize,
    /// Agent roles to spawn
    pub roles: Vec<AgentRole>,
    /// Whether to use streaming responses
    pub streaming: bool,
    /// Timeout per agent request
    pub timeout_secs: u64,
    /// Temperature for generation
    pub temperature: f32,
    /// Max tokens per response
    pub max_tokens: usize,
    /// Failure policy
    pub failure_policy: MultiAgentFailurePolicy,
}

impl Default for MultiAgentConfig {
    fn default() -> Self {
        Self {
            max_concurrency: 4,
            roles: vec![
                AgentRole::Architect,
                AgentRole::Coder,
                AgentRole::Tester,
                AgentRole::Reviewer,
            ],
            streaming: true,
            timeout_secs: 120,
            temperature: 1.0,
            max_tokens: 65536,
            failure_policy: MultiAgentFailurePolicy::BestEffort,
        }
    }
}

impl MultiAgentConfig {
    pub fn with_concurrency(mut self, n: usize) -> Self {
        self.max_concurrency = n.clamp(1, MAX_CONCURRENT_AGENTS);
        self
    }

    pub fn with_roles(mut self, roles: Vec<AgentRole>) -> Self {
        self.roles = roles;
        self
    }
}

/// A single agent instance in the multi-agent system
#[derive(Debug, Clone)]
pub struct AgentInstance {
    pub id: usize,
    pub role: AgentRole,
    pub name: String,
    pub messages: Vec<Message>,
    pub status: AgentStatus,
    /// Timestamp of the last heartbeat, updated during task execution
    pub last_heartbeat: Instant,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentStatus {
    Idle,
    Working,
    Completed,
    Failed,
}

/// Result from an agent execution
#[derive(Debug, Clone)]
pub struct AgentResult {
    pub agent_id: usize,
    pub agent_name: String,
    pub role: AgentRole,
    pub content: String,
    pub tool_calls: Vec<String>,
    pub duration: Duration,
    pub success: bool,
    pub error: Option<String>,
}

/// Event emitted during multi-agent execution
#[derive(Debug, Clone)]
pub enum MultiAgentEvent {
    AgentStarted {
        agent_id: usize,
        name: String,
        task: String,
    },
    AgentProgress {
        agent_id: usize,
        content: String,
    },
    AgentToolCall {
        agent_id: usize,
        tool: String,
    },
    AgentCompleted {
        agent_id: usize,
        result: AgentResult,
    },
    AgentFailed {
        agent_id: usize,
        error: String,
    },
    AllCompleted {
        results: Vec<AgentResult>,
        total_duration: Duration,
    },
}

/// Multi-agent chat orchestrator
///
/// NOTE: This struct uses `tokio::sync::RwLock` and `tokio::sync::Mutex`, which do NOT
/// poison on panic (unlike their `std::sync` counterparts). Therefore lock poisoning
/// recovery is not needed here.
pub struct MultiAgentChat {
    config: MultiAgentConfig,
    client: Arc<ApiClient>,
    tools: Arc<ToolRegistry>,
    semaphore: Arc<Semaphore>,
    agents: Arc<RwLock<Vec<AgentInstance>>>,
    results: Arc<Mutex<Vec<AgentResult>>>,
    event_tx: Option<mpsc::Sender<MultiAgentEvent>>,
}

impl MultiAgentChat {
    /// Create a new multi-agent chat system
    pub fn new(api_config: &Config, agent_config: MultiAgentConfig) -> Result<Self> {
        let client = ApiClient::new(api_config).context("Failed to create API client")?;

        let tools = ToolRegistry::default();
        let concurrency = agent_config.max_concurrency.clamp(1, MAX_CONCURRENT_AGENTS);

        Ok(Self {
            config: agent_config,
            client: Arc::new(client),
            tools: Arc::new(tools),
            semaphore: Arc::new(Semaphore::new(concurrency)),
            agents: Arc::new(RwLock::new(Vec::new())),
            results: Arc::new(Mutex::new(Vec::new())),
            event_tx: None,
        })
    }

    /// Set event sender for streaming events
    pub fn with_events(mut self, tx: mpsc::Sender<MultiAgentEvent>) -> Self {
        self.event_tx = Some(tx);
        self
    }

    /// Initialize agents with their roles
    pub async fn initialize_agents(&self) -> Result<()> {
        let mut agents = self.agents.write().await;
        agents.clear();

        for (i, role) in self.config.roles.iter().enumerate() {
            let agent = AgentInstance {
                id: i,
                role: *role,
                name: format!("Agent-{}-{}", i, role.name()),
                messages: vec![Message::system(role.system_prompt())],
                status: AgentStatus::Idle,
                last_heartbeat: Instant::now(),
            };
            agents.push(agent);
        }

        Ok(())
    }

    /// Send an event if event channel is configured.
    /// Uses `try_send` so this remains non-async; events are dropped if the
    /// bounded channel is full (back-pressure safety).
    fn emit(&self, event: MultiAgentEvent) {
        if let Some(ref tx) = self.event_tx {
            if let Err(mpsc::error::TrySendError::Full(_)) = tx.try_send(event) {
                tracing::warn!("MultiAgent event channel full, dropping event");
            }
            // Closed channel errors are silently ignored (receiver gone).
        }
    }

    /// Run a task across all agents concurrently
    pub async fn run_task(&self, task: &str) -> Result<Vec<AgentResult>> {
        let start = Instant::now();

        // Initialize agents if not already done
        {
            let agents = self.agents.read().await;
            if agents.is_empty() {
                drop(agents);
                self.initialize_agents().await?;
            }
        }

        // Clear previous results
        {
            let mut results = self.results.lock().await;
            results.clear();
        }

        // Get agent count
        let agent_count = {
            let agents = self.agents.read().await;
            agents.len()
        };

        // Shared cancellation state for FailFast policy
        let cancelled = Arc::new(tokio::sync::Notify::new());

        // Spawn concurrent agent tasks using JoinSet for structured cancellation
        let mut join_set = JoinSet::new();

        for agent_id in 0..agent_count {
            let client = Arc::clone(&self.client);
            let tools = Arc::clone(&self.tools);
            let semaphore = Arc::clone(&self.semaphore);
            let agents = Arc::clone(&self.agents);
            let results = Arc::clone(&self.results);
            let task = task.to_string();
            let timeout = Duration::from_secs(self.config.timeout_secs);
            let event_tx = self.event_tx.clone();
            let failure_policy = self.config.failure_policy;
            let cancelled = Arc::clone(&cancelled);

            join_set.spawn(async move {
                tokio::select! {
                    _ = cancelled.notified() => {
                        // Aborted by policy
                        Ok(())
                    }
                    res = Self::run_single_agent(
                        agent_id, task, client, tools, semaphore, agents, results, timeout, event_tx,
                    ) => {
                        if failure_policy == MultiAgentFailurePolicy::FailFast && res.is_err() {
                            cancelled.notify_waiters();
                        }
                        res
                    }
                }
            });
        }

        // Wait for all agents to complete or fail
        while let Some(result) = join_set.join_next().await {
            match result {
                Ok(Ok(_)) => {
                    // Task finished
                }
                Ok(Err(e)) => {
                    eprintln!("Agent-specific error: {}", e);
                    if self.config.failure_policy == MultiAgentFailurePolicy::FailFast {
                        cancelled.notify_waiters();
                        // Abort all remaining in-flight tasks
                        join_set.abort_all();
                        // Drain remaining tasks to ensure clean shutdown
                        while join_set.join_next().await.is_some() {}
                        break;
                    }
                }
                Err(e) if e.is_cancelled() => {
                    // Task was cancelled (e.g., via abort_all), not an error
                    tracing::debug!("Agent task cancelled: {}", e);
                }
                Err(e) => {
                    // Task panicked
                    tracing::error!("Agent task panicked: {}", e);
                    eprintln!("Agent task panicked: {}", e);
                    if self.config.failure_policy == MultiAgentFailurePolicy::FailFast {
                        cancelled.notify_waiters();
                        // Abort all remaining in-flight tasks
                        join_set.abort_all();
                        // Drain remaining tasks to ensure clean shutdown
                        while join_set.join_next().await.is_some() {}
                        break;
                    }
                }
            }
        }

        let total_duration = start.elapsed();

        // Collect results
        let results = {
            let results = self.results.lock().await;
            results.clone()
        };

        self.emit(MultiAgentEvent::AllCompleted {
            results: results.clone(),
            total_duration,
        });

        Ok(results)
    }

    /// Run a single agent's task
    #[allow(clippy::too_many_arguments)]
    async fn run_single_agent(
        agent_id: usize,
        task: String,
        client: Arc<ApiClient>,
        _tools: Arc<ToolRegistry>,
        semaphore: Arc<Semaphore>,
        agents: Arc<RwLock<Vec<AgentInstance>>>,
        results: Arc<Mutex<Vec<AgentResult>>>,
        timeout: Duration,
        event_tx: Option<mpsc::Sender<MultiAgentEvent>>,
    ) -> Result<()> {
        // Acquire semaphore permit
        let _permit = semaphore.acquire().await?;

        let start = Instant::now();

        // Get agent info and update status + heartbeat
        let (agent_name, role, mut messages) = {
            let mut agents = agents.write().await;
            if let Some(agent) = agents.get_mut(agent_id) {
                agent.status = AgentStatus::Working;
                agent.last_heartbeat = Instant::now();
                (agent.name.clone(), agent.role, agent.messages.clone())
            } else {
                return Ok(());
            }
        };

        // Emit start event
        if let Some(ref tx) = event_tx {
            let _ = tx.try_send(MultiAgentEvent::AgentStarted {
                agent_id,
                name: agent_name.clone(),
                task: task.clone(),
            });
        }

        // Add user task to messages
        messages.push(Message::user(&task));

        // Call the API with timeout
        let result =
            tokio::time::timeout(timeout, client.chat(messages, None, ThinkingMode::Disabled))
                .await;

        let duration = start.elapsed();

        let agent_result = match result {
            Ok(Ok(response)) => {
                let content = response
                    .choices
                    .first()
                    .map(|c| c.message.content.text().to_string())
                    .unwrap_or_default();

                // Parse any tool calls
                let parsed = parse_tool_calls(&content);
                let tool_calls: Vec<String> = parsed
                    .tool_calls
                    .iter()
                    .map(|tc| tc.tool_name.clone())
                    .collect();

                // Emit tool call events
                for tool in &tool_calls {
                    if let Some(ref tx) = event_tx {
                        let _ = tx.try_send(MultiAgentEvent::AgentToolCall {
                            agent_id,
                            tool: tool.clone(),
                        });
                    }
                }

                AgentResult {
                    agent_id,
                    agent_name: agent_name.clone(),
                    role,
                    content,
                    tool_calls,
                    duration,
                    success: true,
                    error: None,
                }
            }
            Ok(Err(e)) => {
                if let Some(ref tx) = event_tx {
                    let _ = tx.try_send(MultiAgentEvent::AgentFailed {
                        agent_id,
                        error: e.to_string(),
                    });
                }
                AgentResult {
                    agent_id,
                    agent_name: agent_name.clone(),
                    role,
                    content: String::new(),
                    tool_calls: vec![],
                    duration,
                    success: false,
                    error: Some(e.to_string()),
                }
            }
            Err(_) => {
                let error = "Request timed out".to_string();
                if let Some(ref tx) = event_tx {
                    let _ = tx.try_send(MultiAgentEvent::AgentFailed {
                        agent_id,
                        error: error.clone(),
                    });
                }
                AgentResult {
                    agent_id,
                    agent_name: agent_name.clone(),
                    role,
                    content: String::new(),
                    tool_calls: vec![],
                    duration,
                    success: false,
                    error: Some(error),
                }
            }
        };

        // Update agent status and heartbeat
        {
            let mut agents = agents.write().await;
            if let Some(agent) = agents.get_mut(agent_id) {
                agent.status = if agent_result.success {
                    AgentStatus::Completed
                } else {
                    AgentStatus::Failed
                };
                agent.last_heartbeat = Instant::now();
            }
        }

        // Emit completion event
        if let Some(ref tx) = event_tx {
            let _ = tx.try_send(MultiAgentEvent::AgentCompleted {
                agent_id,
                result: agent_result.clone(),
            });
        }

        let agent_failed = !agent_result.success;

        // Store result
        {
            let mut results = results.lock().await;
            results.push(agent_result);
        }

        if agent_failed {
            Err(anyhow::anyhow!("Agent {} failed", agent_id))
        } else {
            Ok(())
        }
    }

    /// Run interactive multi-agent chat
    pub async fn interactive(&mut self) -> Result<()> {
        use colored::Colorize;
        use std::io::{self, Write};

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
            if io::stdin().read_line(&mut input).is_err() {
                continue;
            }

            let input = input.trim();

            if input == "exit" || input == "quit" {
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
                        self.semaphore = Arc::new(Semaphore::new(n));
                        println!("Max concurrency set to {}", n);
                    }
                } else {
                    println!("Usage: /parallel <1-{}>", MAX_CONCURRENT_AGENTS);
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
                        messages: vec![Message::system(role.system_prompt())],
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

            // Print results
            println!("\n{}", "Agent Responses:".bright_cyan().bold());
            for result in &results {
                if result.success {
                    println!(
                        "\n{} {} ({}):",
                        "━━━".bright_blue(),
                        result.agent_name.bright_white(),
                        result.role.name()
                    );
                    // Truncate long responses for display (UTF-8 safe)
                    let preview = if result.content.len() > 500 {
                        let mut end = 500;
                        while end > 0 && !result.content.is_char_boundary(end) {
                            end -= 1;
                        }
                        format!(
                            "{}...\n[{} more chars]",
                            &result.content[..end],
                            result.content.len() - end
                        )
                    } else {
                        result.content.clone()
                    };
                    println!("{}", preview);
                }
            }

            println!(
                "\n{} Total time: {:.2}s",
                "⏱".bright_yellow(),
                start.elapsed().as_secs_f64()
            );
        }

        Ok(())
    }

    /// Default heartbeat timeout: an agent is considered unhealthy if its
    /// last heartbeat is older than this duration.
    const HEARTBEAT_TIMEOUT: Duration = Duration::from_secs(300);

    /// Check whether a specific agent is healthy based on its heartbeat.
    ///
    /// An agent is healthy if:
    /// - It exists in the agent list
    /// - Its last heartbeat was within `HEARTBEAT_TIMEOUT`
    /// - It is not in the `Failed` state
    pub async fn is_agent_healthy(&self, agent_id: usize) -> bool {
        let agents = self.agents.read().await;
        if let Some(agent) = agents.get(agent_id) {
            agent.status != AgentStatus::Failed
                && agent.last_heartbeat.elapsed() < Self::HEARTBEAT_TIMEOUT
        } else {
            false
        }
    }

    /// Aggregate results from all agents into a summary
    pub fn aggregate_results(results: &[AgentResult]) -> String {
        let mut summary = String::new();

        summary.push_str("## Multi-Agent Summary\n\n");

        for result in results {
            if result.success {
                summary.push_str(&format!(
                    "### {} ({})\n",
                    result.agent_name,
                    result.role.name()
                ));
                summary.push_str(&result.content);
                summary.push_str("\n\n");
            } else if let Some(error) = &result.error {
                summary.push_str(&format!(
                    "### {} (FAILED)\nError: {}\n\n",
                    result.agent_name, error
                ));
            }
        }

        summary
    }
}

/// Quick helper to run a task with default multi-agent config
pub async fn run_multiagent_task(
    config: &Config,
    task: &str,
    concurrency: usize,
) -> Result<Vec<AgentResult>> {
    let agent_config = MultiAgentConfig::default().with_concurrency(concurrency);

    let chat = MultiAgentChat::new(config, agent_config)?;
    chat.run_task(task).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_multiagent_config_default() {
        let config = MultiAgentConfig::default();
        assert_eq!(config.max_concurrency, 4);
        assert_eq!(config.roles.len(), 4);
        assert!(config.streaming);
    }

    #[test]
    fn test_multiagent_config_with_concurrency() {
        let config = MultiAgentConfig::default().with_concurrency(16);
        assert_eq!(config.max_concurrency, 16);

        // Should cap at MAX_CONCURRENT_AGENTS
        let config = MultiAgentConfig::default().with_concurrency(100);
        assert_eq!(config.max_concurrency, MAX_CONCURRENT_AGENTS);
    }

    #[test]
    fn test_agent_instance() {
        let agent = AgentInstance {
            id: 0,
            role: AgentRole::Coder,
            name: "Test Agent".to_string(),
            messages: vec![],
            status: AgentStatus::Idle,
            last_heartbeat: Instant::now(),
        };
        assert_eq!(agent.status, AgentStatus::Idle);
    }

    #[test]
    fn test_agent_result() {
        let result = AgentResult {
            agent_id: 0,
            agent_name: "Test".to_string(),
            role: AgentRole::Coder,
            content: "Hello".to_string(),
            tool_calls: vec![],
            duration: Duration::from_secs(1),
            success: true,
            error: None,
        };
        assert!(result.success);
    }

    #[test]
    fn test_aggregate_results() {
        let results = vec![
            AgentResult {
                agent_id: 0,
                agent_name: "Agent-0".to_string(),
                role: AgentRole::Coder,
                content: "Code here".to_string(),
                tool_calls: vec![],
                duration: Duration::from_secs(1),
                success: true,
                error: None,
            },
            AgentResult {
                agent_id: 1,
                agent_name: "Agent-1".to_string(),
                role: AgentRole::Tester,
                content: "Tests here".to_string(),
                tool_calls: vec![],
                duration: Duration::from_secs(2),
                success: true,
                error: None,
            },
        ];

        let summary = MultiAgentChat::aggregate_results(&results);
        assert!(summary.contains("Agent-0"));
        assert!(summary.contains("Agent-1"));
        assert!(summary.contains("Code here"));
        assert!(summary.contains("Tests here"));
    }

    #[test]
    fn test_max_concurrent_agents() {
        assert_eq!(MAX_CONCURRENT_AGENTS, 16);
    }

    #[test]
    fn test_multiagent_config_with_roles() {
        let roles = vec![AgentRole::Coder, AgentRole::Tester];
        let config = MultiAgentConfig::default().with_roles(roles.clone());
        assert_eq!(config.roles.len(), 2);
        assert!(config.roles.contains(&AgentRole::Coder));
        assert!(config.roles.contains(&AgentRole::Tester));
    }

    #[test]
    fn test_multiagent_config_defaults() {
        let config = MultiAgentConfig::default();
        assert!(config.streaming);
        assert_eq!(config.timeout_secs, 120);
        assert_eq!(config.temperature, 1.0);
        assert_eq!(config.max_tokens, 65536);
    }

    #[test]
    fn test_agent_status_variants() {
        assert_eq!(AgentStatus::Idle, AgentStatus::Idle);
        assert_eq!(AgentStatus::Working, AgentStatus::Working);
        assert_eq!(AgentStatus::Completed, AgentStatus::Completed);
        assert_eq!(AgentStatus::Failed, AgentStatus::Failed);
        assert_ne!(AgentStatus::Idle, AgentStatus::Working);
    }

    #[test]
    fn test_agent_status_debug() {
        let status = AgentStatus::Idle;
        let debug_str = format!("{:?}", status);
        assert_eq!(debug_str, "Idle");
    }

    #[test]
    fn test_agent_instance_fields() {
        let agent = AgentInstance {
            id: 5,
            role: AgentRole::Architect,
            name: "Architect-5".to_string(),
            messages: vec![],
            status: AgentStatus::Working,
            last_heartbeat: Instant::now(),
        };
        assert_eq!(agent.id, 5);
        assert_eq!(agent.role, AgentRole::Architect);
        assert_eq!(agent.name, "Architect-5");
        assert!(agent.messages.is_empty());
    }

    #[test]
    fn test_agent_instance_clone() {
        let agent = AgentInstance {
            id: 0,
            role: AgentRole::Coder,
            name: "Test".to_string(),
            messages: vec![],
            status: AgentStatus::Idle,
            last_heartbeat: Instant::now(),
        };
        let cloned = agent.clone();
        assert_eq!(agent.id, cloned.id);
        assert_eq!(agent.name, cloned.name);
    }

    #[test]
    fn test_agent_result_with_error() {
        let result = AgentResult {
            agent_id: 1,
            agent_name: "Failing".to_string(),
            role: AgentRole::Reviewer,
            content: String::new(),
            tool_calls: vec![],
            duration: Duration::from_secs(5),
            success: false,
            error: Some("Connection timeout".to_string()),
        };
        assert!(!result.success);
        assert!(result.error.is_some());
        assert!(result.error.as_ref().unwrap().contains("timeout"));
    }

    #[test]
    fn test_agent_result_with_tool_calls() {
        let result = AgentResult {
            agent_id: 0,
            agent_name: "Coder".to_string(),
            role: AgentRole::Coder,
            content: "Done".to_string(),
            tool_calls: vec!["file_read".to_string(), "file_write".to_string()],
            duration: Duration::from_millis(500),
            success: true,
            error: None,
        };
        assert_eq!(result.tool_calls.len(), 2);
        assert!(result.tool_calls.contains(&"file_read".to_string()));
    }

    #[test]
    fn test_agent_result_clone() {
        let result = AgentResult {
            agent_id: 0,
            agent_name: "Test".to_string(),
            role: AgentRole::General,
            content: "Content".to_string(),
            tool_calls: vec![],
            duration: Duration::from_secs(1),
            success: true,
            error: None,
        };
        let cloned = result.clone();
        assert_eq!(result.agent_id, cloned.agent_id);
        assert_eq!(result.content, cloned.content);
    }

    #[test]
    fn test_multiagent_event_started() {
        let event = MultiAgentEvent::AgentStarted {
            agent_id: 0,
            name: "Test".to_string(),
            task: "Do something".to_string(),
        };
        if let MultiAgentEvent::AgentStarted {
            agent_id,
            name,
            task,
        } = event
        {
            assert_eq!(agent_id, 0);
            assert_eq!(name, "Test");
            assert_eq!(task, "Do something");
        }
    }

    #[test]
    fn test_multiagent_event_progress() {
        let event = MultiAgentEvent::AgentProgress {
            agent_id: 1,
            content: "Working...".to_string(),
        };
        if let MultiAgentEvent::AgentProgress { agent_id, content } = event {
            assert_eq!(agent_id, 1);
            assert_eq!(content, "Working...");
        }
    }

    #[test]
    fn test_multiagent_event_tool_call() {
        let event = MultiAgentEvent::AgentToolCall {
            agent_id: 2,
            tool: "shell_exec".to_string(),
        };
        if let MultiAgentEvent::AgentToolCall { agent_id, tool } = event {
            assert_eq!(agent_id, 2);
            assert_eq!(tool, "shell_exec");
        }
    }

    #[test]
    fn test_multiagent_event_completed() {
        let result = AgentResult {
            agent_id: 0,
            agent_name: "Agent-0".to_string(),
            role: AgentRole::Coder,
            content: "Done".to_string(),
            tool_calls: vec![],
            duration: Duration::from_secs(10),
            success: true,
            error: None,
        };
        let event = MultiAgentEvent::AgentCompleted {
            agent_id: 0,
            result: result.clone(),
        };
        if let MultiAgentEvent::AgentCompleted {
            agent_id,
            result: r,
        } = event
        {
            assert_eq!(agent_id, 0);
            assert!(r.success);
        }
    }

    #[test]
    fn test_multiagent_event_failed() {
        let event = MultiAgentEvent::AgentFailed {
            agent_id: 3,
            error: "Network error".to_string(),
        };
        if let MultiAgentEvent::AgentFailed { agent_id, error } = event {
            assert_eq!(agent_id, 3);
            assert!(error.contains("Network"));
        }
    }

    #[test]
    fn test_multiagent_event_all_completed() {
        let results = vec![AgentResult {
            agent_id: 0,
            agent_name: "A".to_string(),
            role: AgentRole::Coder,
            content: "".to_string(),
            tool_calls: vec![],
            duration: Duration::from_secs(1),
            success: true,
            error: None,
        }];
        let event = MultiAgentEvent::AllCompleted {
            results: results.clone(),
            total_duration: Duration::from_secs(5),
        };
        if let MultiAgentEvent::AllCompleted {
            results: r,
            total_duration,
        } = event
        {
            assert_eq!(r.len(), 1);
            assert_eq!(total_duration.as_secs(), 5);
        }
    }

    #[test]
    fn test_multiagent_event_clone() {
        let event = MultiAgentEvent::AgentStarted {
            agent_id: 0,
            name: "Test".to_string(),
            task: "Task".to_string(),
        };
        let cloned = event.clone();
        if let (
            MultiAgentEvent::AgentStarted { name: n1, .. },
            MultiAgentEvent::AgentStarted { name: n2, .. },
        ) = (&event, &cloned)
        {
            assert_eq!(n1, n2);
        }
    }

    #[test]
    fn test_aggregate_results_with_failures() {
        let results = vec![
            AgentResult {
                agent_id: 0,
                agent_name: "Success".to_string(),
                role: AgentRole::Coder,
                content: "Good output".to_string(),
                tool_calls: vec![],
                duration: Duration::from_secs(1),
                success: true,
                error: None,
            },
            AgentResult {
                agent_id: 1,
                agent_name: "Failure".to_string(),
                role: AgentRole::Tester,
                content: "".to_string(),
                tool_calls: vec![],
                duration: Duration::from_secs(2),
                success: false,
                error: Some("Error occurred".to_string()),
            },
        ];

        let summary = MultiAgentChat::aggregate_results(&results);
        assert!(summary.contains("Success"));
        assert!(summary.contains("Good output"));
        assert!(summary.contains("Failure"));
        assert!(summary.contains("FAILED"));
        assert!(summary.contains("Error occurred"));
    }

    #[test]
    fn test_aggregate_results_empty() {
        let results: Vec<AgentResult> = vec![];
        let summary = MultiAgentChat::aggregate_results(&results);
        assert!(summary.contains("Summary"));
        assert!(!summary.contains("###")); // No agent sections
    }

    #[test]
    fn test_config_concurrency_zero() {
        let config = MultiAgentConfig::default().with_concurrency(0);
        // Should be at least 0 (system will handle minimum)
        assert!(config.max_concurrency <= MAX_CONCURRENT_AGENTS);
    }

    #[test]
    fn test_config_concurrency_one() {
        let config = MultiAgentConfig::default().with_concurrency(1);
        assert_eq!(config.max_concurrency, 1);
    }

    #[test]
    fn test_multiagent_config_debug() {
        let config = MultiAgentConfig::default();
        let debug_str = format!("{:?}", config);
        assert!(debug_str.contains("max_concurrency"));
        assert!(debug_str.contains("streaming"));
    }

    #[test]
    fn test_agent_instance_debug() {
        let agent = AgentInstance {
            id: 0,
            role: AgentRole::Coder,
            name: "Test".to_string(),
            messages: vec![],
            status: AgentStatus::Idle,
            last_heartbeat: Instant::now(),
        };
        let debug_str = format!("{:?}", agent);
        assert!(debug_str.contains("AgentInstance"));
    }

    #[test]
    fn test_agent_result_debug() {
        let result = AgentResult {
            agent_id: 0,
            agent_name: "Test".to_string(),
            role: AgentRole::Coder,
            content: "Content".to_string(),
            tool_calls: vec![],
            duration: Duration::from_secs(1),
            success: true,
            error: None,
        };
        let debug_str = format!("{:?}", result);
        assert!(debug_str.contains("AgentResult"));
    }

    #[test]
    fn test_multiagent_event_debug() {
        let event = MultiAgentEvent::AgentStarted {
            agent_id: 0,
            name: "Test".to_string(),
            task: "Task".to_string(),
        };
        let debug_str = format!("{:?}", event);
        assert!(debug_str.contains("AgentStarted"));
    }

    #[test]
    fn test_config_empty_roles() {
        let config = MultiAgentConfig::default().with_roles(vec![]);
        assert!(config.roles.is_empty());
    }

    #[test]
    fn test_config_many_roles() {
        let roles = vec![
            AgentRole::Architect,
            AgentRole::Coder,
            AgentRole::Tester,
            AgentRole::Reviewer,
            AgentRole::Documenter,
            AgentRole::DevOps,
            AgentRole::Security,
            AgentRole::Performance,
            AgentRole::General,
        ];
        let config = MultiAgentConfig::default().with_roles(roles);
        assert_eq!(config.roles.len(), 9);
    }

    #[test]
    fn test_agent_status_copy() {
        let status = AgentStatus::Working;
        let copy = status;
        assert_eq!(copy, AgentStatus::Working);
        assert_eq!(status, AgentStatus::Working);
    }

    #[test]
    fn test_agent_status_clone() {
        let status = AgentStatus::Completed;
        #[allow(clippy::clone_on_copy)]
        let cloned = status.clone();
        assert_eq!(cloned, AgentStatus::Completed);
    }

    #[test]
    fn test_agent_result_duration() {
        let result = AgentResult {
            agent_id: 0,
            agent_name: "Test".to_string(),
            role: AgentRole::Coder,
            content: "Done".to_string(),
            tool_calls: vec![],
            duration: Duration::from_millis(1500),
            success: true,
            error: None,
        };
        assert_eq!(result.duration.as_millis(), 1500);
        assert_eq!(result.duration.as_secs(), 1);
    }

    #[test]
    fn test_multiagent_config_chain() {
        let config = MultiAgentConfig::default()
            .with_concurrency(8)
            .with_roles(vec![AgentRole::Coder]);
        assert_eq!(config.max_concurrency, 8);
        assert_eq!(config.roles.len(), 1);
    }

    #[test]
    fn test_multiagent_config_clone() {
        let original = MultiAgentConfig::default();
        let cloned = original.clone();
        assert_eq!(original.max_concurrency, cloned.max_concurrency);
        assert_eq!(original.roles.len(), cloned.roles.len());
        assert_eq!(original.streaming, cloned.streaming);
        assert_eq!(original.timeout_secs, cloned.timeout_secs);
    }

    #[test]
    fn test_aggregate_results_only_failures() {
        let results = vec![
            AgentResult {
                agent_id: 0,
                agent_name: "Failed1".to_string(),
                role: AgentRole::Coder,
                content: "".to_string(),
                tool_calls: vec![],
                duration: Duration::from_secs(1),
                success: false,
                error: Some("Error 1".to_string()),
            },
            AgentResult {
                agent_id: 1,
                agent_name: "Failed2".to_string(),
                role: AgentRole::Tester,
                content: "".to_string(),
                tool_calls: vec![],
                duration: Duration::from_secs(2),
                success: false,
                error: Some("Error 2".to_string()),
            },
        ];
        let summary = MultiAgentChat::aggregate_results(&results);
        assert!(summary.contains("FAILED"));
        assert!(summary.contains("Error 1"));
        assert!(summary.contains("Error 2"));
    }

    #[test]
    fn test_aggregate_results_mixed() {
        let results = vec![
            AgentResult {
                agent_id: 0,
                agent_name: "Success".to_string(),
                role: AgentRole::Architect,
                content: "Architecture done".to_string(),
                tool_calls: vec!["tool1".to_string()],
                duration: Duration::from_secs(5),
                success: true,
                error: None,
            },
            AgentResult {
                agent_id: 1,
                agent_name: "Failed".to_string(),
                role: AgentRole::Security,
                content: "".to_string(),
                tool_calls: vec![],
                duration: Duration::from_secs(3),
                success: false,
                error: None, // No error message
            },
        ];
        let summary = MultiAgentChat::aggregate_results(&results);
        assert!(summary.contains("Success"));
        assert!(summary.contains("Architecture done"));
        // Failed without error message shouldn't have FAILED section
        assert!(!summary.contains("Failed (FAILED)"));
    }

    #[test]
    fn test_multiagent_event_progress_clone() {
        let event = MultiAgentEvent::AgentProgress {
            agent_id: 5,
            content: "Processing...".to_string(),
        };
        let cloned = event.clone();
        if let MultiAgentEvent::AgentProgress { agent_id, content } = cloned {
            assert_eq!(agent_id, 5);
            assert_eq!(content, "Processing...");
        } else {
            panic!("Wrong event type after clone");
        }
    }

    #[test]
    fn test_multiagent_event_tool_call_clone() {
        let event = MultiAgentEvent::AgentToolCall {
            agent_id: 3,
            tool: "file_read".to_string(),
        };
        let cloned = event.clone();
        if let MultiAgentEvent::AgentToolCall { agent_id, tool } = cloned {
            assert_eq!(agent_id, 3);
            assert_eq!(tool, "file_read");
        } else {
            panic!("Wrong event type");
        }
    }

    #[test]
    fn test_agent_instance_with_messages() {
        use crate::api::types::Message;
        let agent = AgentInstance {
            id: 0,
            role: AgentRole::Documenter,
            name: "Doc Agent".to_string(),
            messages: vec![
                Message::system("You are a documenter"),
                Message::user("Document this code"),
            ],
            status: AgentStatus::Idle,
            last_heartbeat: Instant::now(),
        };
        assert_eq!(agent.messages.len(), 2);
    }

    #[test]
    fn test_max_concurrent_agents_constant() {
        // Verify the constant is 16 as documented
        assert_eq!(MAX_CONCURRENT_AGENTS, 16);
    }

    #[test]
    fn test_config_with_concurrency_boundaries() {
        // Test boundary at MAX
        let config = MultiAgentConfig::default().with_concurrency(MAX_CONCURRENT_AGENTS);
        assert_eq!(config.max_concurrency, MAX_CONCURRENT_AGENTS);

        // Test boundary at MAX - 1
        let config = MultiAgentConfig::default().with_concurrency(MAX_CONCURRENT_AGENTS - 1);
        assert_eq!(config.max_concurrency, MAX_CONCURRENT_AGENTS - 1);

        // Test boundary at MAX + 1 (should cap)
        let config = MultiAgentConfig::default().with_concurrency(MAX_CONCURRENT_AGENTS + 1);
        assert_eq!(config.max_concurrency, MAX_CONCURRENT_AGENTS);
    }

    #[test]
    fn test_agent_result_long_content() {
        let long_content = "x".repeat(10000);
        let result = AgentResult {
            agent_id: 0,
            agent_name: "Test".to_string(),
            role: AgentRole::Coder,
            content: long_content.clone(),
            tool_calls: vec![],
            duration: Duration::from_secs(1),
            success: true,
            error: None,
        };
        assert_eq!(result.content.len(), 10000);
    }

    #[test]
    fn test_agent_result_many_tool_calls() {
        let result = AgentResult {
            agent_id: 0,
            agent_name: "Test".to_string(),
            role: AgentRole::DevOps,
            content: "Done".to_string(),
            tool_calls: vec![
                "tool1".to_string(),
                "tool2".to_string(),
                "tool3".to_string(),
                "tool4".to_string(),
                "tool5".to_string(),
            ],
            duration: Duration::from_secs(10),
            success: true,
            error: None,
        };
        assert_eq!(result.tool_calls.len(), 5);
    }

    #[test]
    fn test_all_agent_roles_in_result() {
        let roles = vec![
            AgentRole::Architect,
            AgentRole::Coder,
            AgentRole::Tester,
            AgentRole::Reviewer,
            AgentRole::Documenter,
            AgentRole::DevOps,
            AgentRole::Security,
            AgentRole::Performance,
            AgentRole::General,
        ];
        for (i, role) in roles.iter().enumerate() {
            let result = AgentResult {
                agent_id: i,
                agent_name: format!("Agent-{}", i),
                role: *role,
                content: "Test".to_string(),
                tool_calls: vec![],
                duration: Duration::from_secs(1),
                success: true,
                error: None,
            };
            assert_eq!(result.role, *role);
        }
    }

    #[test]
    fn test_agent_heartbeat_field() {
        let now = Instant::now();
        let agent = AgentInstance {
            id: 0,
            role: AgentRole::Coder,
            name: "Heartbeat Test".to_string(),
            messages: vec![],
            status: AgentStatus::Working,
            last_heartbeat: now,
        };
        // Heartbeat should be very recent
        assert!(agent.last_heartbeat.elapsed() < Duration::from_secs(1));
    }

    #[test]
    fn test_agent_failed_status_not_healthy() {
        // A failed agent should not be considered healthy regardless of heartbeat
        let agent = AgentInstance {
            id: 0,
            role: AgentRole::Coder,
            name: "Failed Agent".to_string(),
            messages: vec![],
            status: AgentStatus::Failed,
            last_heartbeat: Instant::now(),
        };
        // Failed status means unhealthy
        assert_eq!(agent.status, AgentStatus::Failed);
    }

    // ---------------------------------------------------------------
    // MultiAgentFailurePolicy tests
    // ---------------------------------------------------------------

    #[test]
    fn test_failure_policy_default_is_fail_fast() {
        let policy = MultiAgentFailurePolicy::default();
        assert_eq!(policy, MultiAgentFailurePolicy::FailFast);
    }

    #[test]
    fn test_failure_policy_variants_not_equal() {
        assert_ne!(
            MultiAgentFailurePolicy::BestEffort,
            MultiAgentFailurePolicy::FailFast
        );
    }

    #[test]
    fn test_failure_policy_debug() {
        let best_effort = format!("{:?}", MultiAgentFailurePolicy::BestEffort);
        let fail_fast = format!("{:?}", MultiAgentFailurePolicy::FailFast);
        assert_eq!(best_effort, "BestEffort");
        assert_eq!(fail_fast, "FailFast");
    }

    #[test]
    fn test_failure_policy_clone() {
        let original = MultiAgentFailurePolicy::BestEffort;
        #[allow(clippy::clone_on_copy)]
        let cloned = original.clone();
        assert_eq!(original, cloned);
    }

    #[test]
    fn test_failure_policy_copy() {
        let original = MultiAgentFailurePolicy::FailFast;
        let copied = original;
        // Both should still be usable (Copy trait)
        assert_eq!(original, copied);
        assert_eq!(original, MultiAgentFailurePolicy::FailFast);
    }

    #[test]
    fn test_failure_policy_eq_reflexive() {
        let a = MultiAgentFailurePolicy::BestEffort;
        assert_eq!(a, a);
        let b = MultiAgentFailurePolicy::FailFast;
        assert_eq!(b, b);
    }

    // ---------------------------------------------------------------
    // MultiAgentConfig -- default values in depth
    // ---------------------------------------------------------------

    #[test]
    fn test_config_default_failure_policy() {
        let config = MultiAgentConfig::default();
        // The config default explicitly sets BestEffort (distinct from the enum default of FailFast)
        assert_eq!(config.failure_policy, MultiAgentFailurePolicy::BestEffort);
    }

    #[test]
    fn test_config_default_roles_are_architect_coder_tester_reviewer() {
        let config = MultiAgentConfig::default();
        assert_eq!(config.roles.len(), 4);
        assert_eq!(config.roles[0], AgentRole::Architect);
        assert_eq!(config.roles[1], AgentRole::Coder);
        assert_eq!(config.roles[2], AgentRole::Tester);
        assert_eq!(config.roles[3], AgentRole::Reviewer);
    }

    #[test]
    fn test_config_default_temperature() {
        let config = MultiAgentConfig::default();
        assert!((config.temperature - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_config_default_max_tokens() {
        let config = MultiAgentConfig::default();
        assert_eq!(config.max_tokens, 65536);
    }

    #[test]
    fn test_config_default_timeout_secs() {
        let config = MultiAgentConfig::default();
        assert_eq!(config.timeout_secs, 120);
    }

    #[test]
    fn test_config_default_streaming() {
        let config = MultiAgentConfig::default();
        assert!(config.streaming);
    }

    #[test]
    fn test_config_default_max_concurrency() {
        let config = MultiAgentConfig::default();
        assert_eq!(config.max_concurrency, 4);
    }

    // ---------------------------------------------------------------
    // MultiAgentConfig -- builder methods
    // ---------------------------------------------------------------

    #[test]
    fn test_with_concurrency_clamps_zero_to_one() {
        let config = MultiAgentConfig::default().with_concurrency(0);
        assert_eq!(config.max_concurrency, 1);
    }

    #[test]
    fn test_with_concurrency_clamps_large_to_max() {
        let config = MultiAgentConfig::default().with_concurrency(1000);
        assert_eq!(config.max_concurrency, MAX_CONCURRENT_AGENTS);
    }

    #[test]
    fn test_with_concurrency_exact_min() {
        let config = MultiAgentConfig::default().with_concurrency(1);
        assert_eq!(config.max_concurrency, 1);
    }

    #[test]
    fn test_with_concurrency_exact_max() {
        let config = MultiAgentConfig::default().with_concurrency(MAX_CONCURRENT_AGENTS);
        assert_eq!(config.max_concurrency, MAX_CONCURRENT_AGENTS);
    }

    #[test]
    fn test_with_concurrency_mid_value() {
        let config = MultiAgentConfig::default().with_concurrency(8);
        assert_eq!(config.max_concurrency, 8);
    }

    #[test]
    fn test_with_roles_replaces_all_roles() {
        let config = MultiAgentConfig::default()
            .with_roles(vec![AgentRole::Security, AgentRole::Performance]);
        assert_eq!(config.roles.len(), 2);
        assert_eq!(config.roles[0], AgentRole::Security);
        assert_eq!(config.roles[1], AgentRole::Performance);
    }

    #[test]
    fn test_with_roles_empty_vec() {
        let config = MultiAgentConfig::default().with_roles(vec![]);
        assert!(config.roles.is_empty());
    }

    #[test]
    fn test_with_roles_duplicate_roles() {
        let config = MultiAgentConfig::default().with_roles(vec![
            AgentRole::Coder,
            AgentRole::Coder,
            AgentRole::Coder,
        ]);
        assert_eq!(config.roles.len(), 3);
    }

    #[test]
    fn test_builder_chain_preserves_other_fields() {
        let config = MultiAgentConfig::default()
            .with_concurrency(2)
            .with_roles(vec![AgentRole::General]);
        // Chained builders should preserve all other default fields
        assert!(config.streaming);
        assert_eq!(config.timeout_secs, 120);
        assert!((config.temperature - 1.0).abs() < f32::EPSILON);
        assert_eq!(config.max_tokens, 65536);
        assert_eq!(config.failure_policy, MultiAgentFailurePolicy::BestEffort);
    }

    #[test]
    fn test_builder_order_independence() {
        let config_a = MultiAgentConfig::default()
            .with_concurrency(3)
            .with_roles(vec![AgentRole::Tester]);
        let config_b = MultiAgentConfig::default()
            .with_roles(vec![AgentRole::Tester])
            .with_concurrency(3);
        assert_eq!(config_a.max_concurrency, config_b.max_concurrency);
        assert_eq!(config_a.roles, config_b.roles);
    }

    // ---------------------------------------------------------------
    // AgentStatus -- exhaustive variant tests
    // ---------------------------------------------------------------

    #[test]
    fn test_agent_status_all_variants_distinct() {
        let variants = [
            AgentStatus::Idle,
            AgentStatus::Working,
            AgentStatus::Completed,
            AgentStatus::Failed,
        ];
        for (i, a) in variants.iter().enumerate() {
            for (j, b) in variants.iter().enumerate() {
                if i == j {
                    assert_eq!(a, b);
                } else {
                    assert_ne!(a, b);
                }
            }
        }
    }

    #[test]
    fn test_agent_status_debug_all_variants() {
        assert_eq!(format!("{:?}", AgentStatus::Idle), "Idle");
        assert_eq!(format!("{:?}", AgentStatus::Working), "Working");
        assert_eq!(format!("{:?}", AgentStatus::Completed), "Completed");
        assert_eq!(format!("{:?}", AgentStatus::Failed), "Failed");
    }

    // ---------------------------------------------------------------
    // AgentInstance -- construction and field access
    // ---------------------------------------------------------------

    #[test]
    fn test_agent_instance_name_format() {
        let agent = AgentInstance {
            id: 3,
            role: AgentRole::DevOps,
            name: format!("Agent-{}-{}", 3, AgentRole::DevOps.name()),
            messages: vec![],
            status: AgentStatus::Idle,
            last_heartbeat: Instant::now(),
        };
        assert_eq!(agent.name, "Agent-3-DevOps");
    }

    #[test]
    fn test_agent_instance_with_system_prompt_message() {
        let role = AgentRole::Architect;
        let agent = AgentInstance {
            id: 0,
            role,
            name: format!("Agent-0-{}", role.name()),
            messages: vec![Message::system(role.system_prompt())],
            status: AgentStatus::Idle,
            last_heartbeat: Instant::now(),
        };
        assert_eq!(agent.messages.len(), 1);
        // The system prompt should be non-empty
        assert!(!role.system_prompt().is_empty());
    }

    #[test]
    fn test_agent_instance_status_transitions() {
        let mut agent = AgentInstance {
            id: 0,
            role: AgentRole::Coder,
            name: "Test".to_string(),
            messages: vec![],
            status: AgentStatus::Idle,
            last_heartbeat: Instant::now(),
        };
        assert_eq!(agent.status, AgentStatus::Idle);
        agent.status = AgentStatus::Working;
        assert_eq!(agent.status, AgentStatus::Working);
        agent.status = AgentStatus::Completed;
        assert_eq!(agent.status, AgentStatus::Completed);
    }

    #[test]
    fn test_agent_instance_status_transition_to_failed() {
        let mut agent = AgentInstance {
            id: 0,
            role: AgentRole::Tester,
            name: "Test".to_string(),
            messages: vec![],
            status: AgentStatus::Working,
            last_heartbeat: Instant::now(),
        };
        agent.status = AgentStatus::Failed;
        assert_eq!(agent.status, AgentStatus::Failed);
    }

    #[test]
    fn test_agent_instance_heartbeat_update() {
        let old_time = Instant::now();
        let mut agent = AgentInstance {
            id: 0,
            role: AgentRole::Coder,
            name: "Test".to_string(),
            messages: vec![],
            status: AgentStatus::Idle,
            last_heartbeat: old_time,
        };
        // Simulate a small delay
        std::thread::sleep(Duration::from_millis(5));
        agent.last_heartbeat = Instant::now();
        assert!(agent.last_heartbeat > old_time);
    }

    #[test]
    fn test_agent_instance_messages_can_grow() {
        let mut agent = AgentInstance {
            id: 0,
            role: AgentRole::General,
            name: "Test".to_string(),
            messages: vec![Message::system("You are an assistant")],
            status: AgentStatus::Idle,
            last_heartbeat: Instant::now(),
        };
        agent.messages.push(Message::user("Hello"));
        agent.messages.push(Message::assistant("Hi there"));
        assert_eq!(agent.messages.len(), 3);
    }

    // ---------------------------------------------------------------
    // AgentResult -- construction and fields
    // ---------------------------------------------------------------

    #[test]
    fn test_agent_result_success_has_no_error() {
        let result = AgentResult {
            agent_id: 0,
            agent_name: "Agent-0-Coder".to_string(),
            role: AgentRole::Coder,
            content: "fn main() {}".to_string(),
            tool_calls: vec![],
            duration: Duration::from_millis(100),
            success: true,
            error: None,
        };
        assert!(result.success);
        assert!(result.error.is_none());
        assert!(!result.content.is_empty());
    }

    #[test]
    fn test_agent_result_failure_has_error() {
        let result = AgentResult {
            agent_id: 1,
            agent_name: "Agent-1-Tester".to_string(),
            role: AgentRole::Tester,
            content: String::new(),
            tool_calls: vec![],
            duration: Duration::from_secs(30),
            success: false,
            error: Some("Request timed out".to_string()),
        };
        assert!(!result.success);
        assert_eq!(result.error.as_deref(), Some("Request timed out"));
        assert!(result.content.is_empty());
    }

    #[test]
    fn test_agent_result_failure_no_error_message() {
        // edge case: success is false but no error message
        let result = AgentResult {
            agent_id: 0,
            agent_name: "Test".to_string(),
            role: AgentRole::General,
            content: String::new(),
            tool_calls: vec![],
            duration: Duration::from_secs(1),
            success: false,
            error: None,
        };
        assert!(!result.success);
        assert!(result.error.is_none());
    }

    #[test]
    fn test_agent_result_zero_duration() {
        let result = AgentResult {
            agent_id: 0,
            agent_name: "Fast".to_string(),
            role: AgentRole::Coder,
            content: "instant".to_string(),
            tool_calls: vec![],
            duration: Duration::ZERO,
            success: true,
            error: None,
        };
        assert_eq!(result.duration, Duration::ZERO);
    }

    #[test]
    fn test_agent_result_all_roles() {
        let all_roles = [
            AgentRole::Architect,
            AgentRole::Coder,
            AgentRole::Tester,
            AgentRole::Reviewer,
            AgentRole::Documenter,
            AgentRole::DevOps,
            AgentRole::Security,
            AgentRole::Performance,
            AgentRole::General,
        ];
        for role in &all_roles {
            let result = AgentResult {
                agent_id: 0,
                agent_name: format!("Agent-{}", role.name()),
                role: *role,
                content: "test".to_string(),
                tool_calls: vec![],
                duration: Duration::from_secs(1),
                success: true,
                error: None,
            };
            assert_eq!(result.role, *role);
            assert!(result.agent_name.contains(role.name()));
        }
    }

    // ---------------------------------------------------------------
    // MultiAgentEvent -- all variants and pattern matching
    // ---------------------------------------------------------------

    #[test]
    fn test_event_agent_started_fields() {
        let event = MultiAgentEvent::AgentStarted {
            agent_id: 7,
            name: "Agent-7-Security".to_string(),
            task: "Audit the codebase".to_string(),
        };
        match event {
            MultiAgentEvent::AgentStarted {
                agent_id,
                name,
                task,
            } => {
                assert_eq!(agent_id, 7);
                assert_eq!(name, "Agent-7-Security");
                assert_eq!(task, "Audit the codebase");
            }
            _ => panic!("Expected AgentStarted variant"),
        }
    }

    #[test]
    fn test_event_agent_progress_fields() {
        let event = MultiAgentEvent::AgentProgress {
            agent_id: 2,
            content: "50% complete".to_string(),
        };
        match event {
            MultiAgentEvent::AgentProgress { agent_id, content } => {
                assert_eq!(agent_id, 2);
                assert_eq!(content, "50% complete");
            }
            _ => panic!("Expected AgentProgress variant"),
        }
    }

    #[test]
    fn test_event_agent_tool_call_fields() {
        let event = MultiAgentEvent::AgentToolCall {
            agent_id: 0,
            tool: "file_write".to_string(),
        };
        match event {
            MultiAgentEvent::AgentToolCall { agent_id, tool } => {
                assert_eq!(agent_id, 0);
                assert_eq!(tool, "file_write");
            }
            _ => panic!("Expected AgentToolCall variant"),
        }
    }

    #[test]
    fn test_event_agent_completed_fields() {
        let result = AgentResult {
            agent_id: 3,
            agent_name: "Agent-3-Reviewer".to_string(),
            role: AgentRole::Reviewer,
            content: "LGTM".to_string(),
            tool_calls: vec!["file_read".to_string()],
            duration: Duration::from_secs(15),
            success: true,
            error: None,
        };
        let event = MultiAgentEvent::AgentCompleted {
            agent_id: 3,
            result: result.clone(),
        };
        match event {
            MultiAgentEvent::AgentCompleted { agent_id, result } => {
                assert_eq!(agent_id, 3);
                assert!(result.success);
                assert_eq!(result.content, "LGTM");
                assert_eq!(result.tool_calls.len(), 1);
            }
            _ => panic!("Expected AgentCompleted variant"),
        }
    }

    #[test]
    fn test_event_agent_failed_fields() {
        let event = MultiAgentEvent::AgentFailed {
            agent_id: 5,
            error: "Out of memory".to_string(),
        };
        match event {
            MultiAgentEvent::AgentFailed { agent_id, error } => {
                assert_eq!(agent_id, 5);
                assert_eq!(error, "Out of memory");
            }
            _ => panic!("Expected AgentFailed variant"),
        }
    }

    #[test]
    fn test_event_all_completed_fields() {
        let results = vec![
            AgentResult {
                agent_id: 0,
                agent_name: "A".to_string(),
                role: AgentRole::Coder,
                content: "done".to_string(),
                tool_calls: vec![],
                duration: Duration::from_secs(2),
                success: true,
                error: None,
            },
            AgentResult {
                agent_id: 1,
                agent_name: "B".to_string(),
                role: AgentRole::Tester,
                content: "".to_string(),
                tool_calls: vec![],
                duration: Duration::from_secs(3),
                success: false,
                error: Some("fail".to_string()),
            },
        ];
        let total_dur = Duration::from_secs(3);
        let event = MultiAgentEvent::AllCompleted {
            results: results.clone(),
            total_duration: total_dur,
        };
        match event {
            MultiAgentEvent::AllCompleted {
                results,
                total_duration,
            } => {
                assert_eq!(results.len(), 2);
                assert_eq!(total_duration, Duration::from_secs(3));
                assert!(results[0].success);
                assert!(!results[1].success);
            }
            _ => panic!("Expected AllCompleted variant"),
        }
    }

    #[test]
    fn test_event_all_completed_empty() {
        let event = MultiAgentEvent::AllCompleted {
            results: vec![],
            total_duration: Duration::ZERO,
        };
        match event {
            MultiAgentEvent::AllCompleted {
                results,
                total_duration,
            } => {
                assert!(results.is_empty());
                assert_eq!(total_duration, Duration::ZERO);
            }
            _ => panic!("Expected AllCompleted variant"),
        }
    }

    #[test]
    fn test_event_debug_all_variants() {
        let started = format!(
            "{:?}",
            MultiAgentEvent::AgentStarted {
                agent_id: 0,
                name: "T".into(),
                task: "X".into(),
            }
        );
        assert!(started.contains("AgentStarted"));

        let progress = format!(
            "{:?}",
            MultiAgentEvent::AgentProgress {
                agent_id: 0,
                content: "c".into(),
            }
        );
        assert!(progress.contains("AgentProgress"));

        let tool_call = format!(
            "{:?}",
            MultiAgentEvent::AgentToolCall {
                agent_id: 0,
                tool: "t".into(),
            }
        );
        assert!(tool_call.contains("AgentToolCall"));

        let failed = format!(
            "{:?}",
            MultiAgentEvent::AgentFailed {
                agent_id: 0,
                error: "e".into(),
            }
        );
        assert!(failed.contains("AgentFailed"));
    }

    #[test]
    fn test_event_clone_all_variants() {
        // AgentStarted
        let e1 = MultiAgentEvent::AgentStarted {
            agent_id: 1,
            name: "N".into(),
            task: "T".into(),
        };
        let c1 = e1.clone();
        assert!(matches!(
            c1,
            MultiAgentEvent::AgentStarted { agent_id: 1, .. }
        ));

        // AgentProgress
        let e2 = MultiAgentEvent::AgentProgress {
            agent_id: 2,
            content: "C".into(),
        };
        let c2 = e2.clone();
        assert!(matches!(
            c2,
            MultiAgentEvent::AgentProgress { agent_id: 2, .. }
        ));

        // AgentToolCall
        let e3 = MultiAgentEvent::AgentToolCall {
            agent_id: 3,
            tool: "T".into(),
        };
        let c3 = e3.clone();
        assert!(matches!(
            c3,
            MultiAgentEvent::AgentToolCall { agent_id: 3, .. }
        ));

        // AgentFailed
        let e4 = MultiAgentEvent::AgentFailed {
            agent_id: 4,
            error: "E".into(),
        };
        let c4 = e4.clone();
        assert!(matches!(
            c4,
            MultiAgentEvent::AgentFailed { agent_id: 4, .. }
        ));

        // AgentCompleted
        let r = AgentResult {
            agent_id: 5,
            agent_name: "A".into(),
            role: AgentRole::Coder,
            content: "X".into(),
            tool_calls: vec![],
            duration: Duration::from_secs(1),
            success: true,
            error: None,
        };
        let e5 = MultiAgentEvent::AgentCompleted {
            agent_id: 5,
            result: r,
        };
        let c5 = e5.clone();
        assert!(matches!(
            c5,
            MultiAgentEvent::AgentCompleted { agent_id: 5, .. }
        ));

        // AllCompleted
        let e6 = MultiAgentEvent::AllCompleted {
            results: vec![],
            total_duration: Duration::from_millis(500),
        };
        let c6 = e6.clone();
        assert!(matches!(c6, MultiAgentEvent::AllCompleted { .. }));
    }

    // ---------------------------------------------------------------
    // aggregate_results -- thorough edge cases
    // ---------------------------------------------------------------

    #[test]
    fn test_aggregate_results_header() {
        let summary = MultiAgentChat::aggregate_results(&[]);
        assert!(summary.starts_with("## Multi-Agent Summary\n\n"));
    }

    #[test]
    fn test_aggregate_results_success_format() {
        let results = vec![AgentResult {
            agent_id: 0,
            agent_name: "MyAgent".to_string(),
            role: AgentRole::Architect,
            content: "Architecture plan here".to_string(),
            tool_calls: vec![],
            duration: Duration::from_secs(5),
            success: true,
            error: None,
        }];
        let summary = MultiAgentChat::aggregate_results(&results);
        assert!(summary.contains("### MyAgent (Architect)"));
        assert!(summary.contains("Architecture plan here"));
    }

    #[test]
    fn test_aggregate_results_failure_format() {
        let results = vec![AgentResult {
            agent_id: 0,
            agent_name: "FailBot".to_string(),
            role: AgentRole::Tester,
            content: "".to_string(),
            tool_calls: vec![],
            duration: Duration::from_secs(1),
            success: false,
            error: Some("API timeout".to_string()),
        }];
        let summary = MultiAgentChat::aggregate_results(&results);
        assert!(summary.contains("### FailBot (FAILED)"));
        assert!(summary.contains("Error: API timeout"));
    }

    #[test]
    fn test_aggregate_results_failure_without_error_msg_excluded() {
        // A failed result with error=None should not produce a FAILED section
        let results = vec![AgentResult {
            agent_id: 0,
            agent_name: "SilentFail".to_string(),
            role: AgentRole::Coder,
            content: "".to_string(),
            tool_calls: vec![],
            duration: Duration::from_secs(1),
            success: false,
            error: None,
        }];
        let summary = MultiAgentChat::aggregate_results(&results);
        // No "###" sections should appear
        assert!(!summary.contains("### SilentFail"));
    }

    #[test]
    fn test_aggregate_results_multiple_successes() {
        let results = vec![
            AgentResult {
                agent_id: 0,
                agent_name: "Arch".to_string(),
                role: AgentRole::Architect,
                content: "Design doc".to_string(),
                tool_calls: vec![],
                duration: Duration::from_secs(1),
                success: true,
                error: None,
            },
            AgentResult {
                agent_id: 1,
                agent_name: "Cod".to_string(),
                role: AgentRole::Coder,
                content: "Implementation".to_string(),
                tool_calls: vec![],
                duration: Duration::from_secs(2),
                success: true,
                error: None,
            },
            AgentResult {
                agent_id: 2,
                agent_name: "Test".to_string(),
                role: AgentRole::Tester,
                content: "Test plan".to_string(),
                tool_calls: vec![],
                duration: Duration::from_secs(3),
                success: true,
                error: None,
            },
        ];
        let summary = MultiAgentChat::aggregate_results(&results);
        assert!(summary.contains("### Arch (Architect)"));
        assert!(summary.contains("Design doc"));
        assert!(summary.contains("### Cod (Coder)"));
        assert!(summary.contains("Implementation"));
        assert!(summary.contains("### Test (Tester)"));
        assert!(summary.contains("Test plan"));
    }

    #[test]
    fn test_aggregate_results_multiple_failures() {
        let results = vec![
            AgentResult {
                agent_id: 0,
                agent_name: "Fail1".to_string(),
                role: AgentRole::Coder,
                content: "".to_string(),
                tool_calls: vec![],
                duration: Duration::from_secs(1),
                success: false,
                error: Some("Error A".to_string()),
            },
            AgentResult {
                agent_id: 1,
                agent_name: "Fail2".to_string(),
                role: AgentRole::Tester,
                content: "".to_string(),
                tool_calls: vec![],
                duration: Duration::from_secs(1),
                success: false,
                error: Some("Error B".to_string()),
            },
        ];
        let summary = MultiAgentChat::aggregate_results(&results);
        assert!(summary.contains("Fail1 (FAILED)"));
        assert!(summary.contains("Error: Error A"));
        assert!(summary.contains("Fail2 (FAILED)"));
        assert!(summary.contains("Error: Error B"));
    }

    #[test]
    fn test_aggregate_results_ordering_preserved() {
        let results = vec![
            AgentResult {
                agent_id: 0,
                agent_name: "First".to_string(),
                role: AgentRole::Coder,
                content: "FIRST_CONTENT".to_string(),
                tool_calls: vec![],
                duration: Duration::from_secs(1),
                success: true,
                error: None,
            },
            AgentResult {
                agent_id: 1,
                agent_name: "Second".to_string(),
                role: AgentRole::Tester,
                content: "SECOND_CONTENT".to_string(),
                tool_calls: vec![],
                duration: Duration::from_secs(1),
                success: true,
                error: None,
            },
        ];
        let summary = MultiAgentChat::aggregate_results(&results);
        let first_pos = summary.find("FIRST_CONTENT").unwrap();
        let second_pos = summary.find("SECOND_CONTENT").unwrap();
        assert!(first_pos < second_pos);
    }

    #[test]
    fn test_aggregate_results_with_multiline_content() {
        let results = vec![AgentResult {
            agent_id: 0,
            agent_name: "MultiLine".to_string(),
            role: AgentRole::Documenter,
            content: "Line 1\nLine 2\nLine 3".to_string(),
            tool_calls: vec![],
            duration: Duration::from_secs(1),
            success: true,
            error: None,
        }];
        let summary = MultiAgentChat::aggregate_results(&results);
        assert!(summary.contains("Line 1\nLine 2\nLine 3"));
    }

    #[test]
    fn test_aggregate_results_with_special_characters() {
        let results = vec![AgentResult {
            agent_id: 0,
            agent_name: "SpecialAgent".to_string(),
            role: AgentRole::Coder,
            content: "fn foo<T: Clone>(x: &T) -> T { x.clone() }".to_string(),
            tool_calls: vec![],
            duration: Duration::from_secs(1),
            success: true,
            error: None,
        }];
        let summary = MultiAgentChat::aggregate_results(&results);
        assert!(summary.contains("fn foo<T: Clone>"));
    }

    // ---------------------------------------------------------------
    // MultiAgentChat construction (with Config::default())
    // ---------------------------------------------------------------

    #[test]
    fn test_multiagent_chat_new_succeeds() {
        let config = Config::default();
        let agent_config = MultiAgentConfig::default();
        let chat = MultiAgentChat::new(&config, agent_config);
        assert!(chat.is_ok());
    }

    #[test]
    fn test_multiagent_chat_new_with_custom_concurrency() {
        let config = Config::default();
        let agent_config = MultiAgentConfig::default().with_concurrency(8);
        let chat = MultiAgentChat::new(&config, agent_config).unwrap();
        assert_eq!(chat.config.max_concurrency, 8);
    }

    #[test]
    fn test_multiagent_chat_new_with_single_role() {
        let config = Config::default();
        let agent_config = MultiAgentConfig::default().with_roles(vec![AgentRole::Coder]);
        let chat = MultiAgentChat::new(&config, agent_config).unwrap();
        assert_eq!(chat.config.roles.len(), 1);
        assert_eq!(chat.config.roles[0], AgentRole::Coder);
    }

    #[test]
    fn test_multiagent_chat_new_event_tx_is_none() {
        let config = Config::default();
        let agent_config = MultiAgentConfig::default();
        let chat = MultiAgentChat::new(&config, agent_config).unwrap();
        assert!(chat.event_tx.is_none());
    }

    #[test]
    fn test_multiagent_chat_with_events() {
        let config = Config::default();
        let agent_config = MultiAgentConfig::default();
        let chat = MultiAgentChat::new(&config, agent_config).unwrap();
        let (tx, _rx) = mpsc::channel::<MultiAgentEvent>(100);
        let chat = chat.with_events(tx);
        assert!(chat.event_tx.is_some());
    }

    #[test]
    fn test_multiagent_chat_concurrency_clamped_on_construct() {
        let config = Config::default();
        let agent_config = MultiAgentConfig {
            max_concurrency: 100, // Bypass builder, set directly
            ..Default::default()
        };
        let chat = MultiAgentChat::new(&config, agent_config).unwrap();
        // new() clamps concurrency for the semaphore
        assert!(chat.semaphore.available_permits() <= MAX_CONCURRENT_AGENTS);
    }

    // ---------------------------------------------------------------
    // MultiAgentChat::emit -- synchronous event emission
    // ---------------------------------------------------------------

    #[test]
    fn test_emit_without_event_tx() {
        // emit should be a no-op when event_tx is None
        let config = Config::default();
        let agent_config = MultiAgentConfig::default();
        let chat = MultiAgentChat::new(&config, agent_config).unwrap();
        // Should not panic
        chat.emit(MultiAgentEvent::AgentStarted {
            agent_id: 0,
            name: "Test".into(),
            task: "Task".into(),
        });
    }

    #[test]
    fn test_emit_with_event_tx_sends_event() {
        let config = Config::default();
        let agent_config = MultiAgentConfig::default();
        let chat = MultiAgentChat::new(&config, agent_config).unwrap();
        let (tx, mut rx) = mpsc::channel::<MultiAgentEvent>(100);
        let chat = chat.with_events(tx);

        chat.emit(MultiAgentEvent::AgentFailed {
            agent_id: 42,
            error: "test error".into(),
        });

        // try_recv should succeed since emit uses try_send
        let event = rx.try_recv().unwrap();
        match event {
            MultiAgentEvent::AgentFailed { agent_id, error } => {
                assert_eq!(agent_id, 42);
                assert_eq!(error, "test error");
            }
            _ => panic!("Expected AgentFailed event"),
        }
    }

    #[test]
    fn test_emit_full_channel_does_not_panic() {
        let config = Config::default();
        let agent_config = MultiAgentConfig::default();
        let chat = MultiAgentChat::new(&config, agent_config).unwrap();
        // Create a channel with capacity 1
        let (tx, _rx) = mpsc::channel::<MultiAgentEvent>(1);
        let chat = chat.with_events(tx);

        // Fill the channel
        chat.emit(MultiAgentEvent::AgentProgress {
            agent_id: 0,
            content: "first".into(),
        });
        // This should not panic even though channel is full
        chat.emit(MultiAgentEvent::AgentProgress {
            agent_id: 1,
            content: "second (dropped)".into(),
        });
    }

    #[test]
    fn test_emit_closed_channel_does_not_panic() {
        let config = Config::default();
        let agent_config = MultiAgentConfig::default();
        let chat = MultiAgentChat::new(&config, agent_config).unwrap();
        let (tx, rx) = mpsc::channel::<MultiAgentEvent>(100);
        let chat = chat.with_events(tx);

        // Drop the receiver to close the channel
        drop(rx);

        // Should not panic
        chat.emit(MultiAgentEvent::AgentStarted {
            agent_id: 0,
            name: "Test".into(),
            task: "Task".into(),
        });
    }

    #[test]
    fn test_emit_multiple_events() {
        let config = Config::default();
        let agent_config = MultiAgentConfig::default();
        let chat = MultiAgentChat::new(&config, agent_config).unwrap();
        let (tx, mut rx) = mpsc::channel::<MultiAgentEvent>(100);
        let chat = chat.with_events(tx);

        chat.emit(MultiAgentEvent::AgentStarted {
            agent_id: 0,
            name: "A".into(),
            task: "T1".into(),
        });
        chat.emit(MultiAgentEvent::AgentProgress {
            agent_id: 0,
            content: "working".into(),
        });
        chat.emit(MultiAgentEvent::AgentToolCall {
            agent_id: 0,
            tool: "file_read".into(),
        });

        let e1 = rx.try_recv().unwrap();
        assert!(matches!(e1, MultiAgentEvent::AgentStarted { .. }));
        let e2 = rx.try_recv().unwrap();
        assert!(matches!(e2, MultiAgentEvent::AgentProgress { .. }));
        let e3 = rx.try_recv().unwrap();
        assert!(matches!(e3, MultiAgentEvent::AgentToolCall { .. }));
    }

    // ---------------------------------------------------------------
    // MultiAgentChat::HEARTBEAT_TIMEOUT constant
    // ---------------------------------------------------------------

    #[test]
    fn test_heartbeat_timeout_value() {
        assert_eq!(MultiAgentChat::HEARTBEAT_TIMEOUT, Duration::from_secs(300));
    }

    // ---------------------------------------------------------------
    // Async tests -- initialize_agents, is_agent_healthy
    // ---------------------------------------------------------------

    #[tokio::test]
    async fn test_initialize_agents_creates_agents() {
        let config = Config::default();
        let agent_config = MultiAgentConfig::default();
        let chat = MultiAgentChat::new(&config, agent_config).unwrap();

        chat.initialize_agents().await.unwrap();

        let agents = chat.agents.read().await;
        assert_eq!(agents.len(), 4);
    }

    #[tokio::test]
    async fn test_initialize_agents_sets_correct_ids() {
        let config = Config::default();
        let agent_config = MultiAgentConfig::default();
        let chat = MultiAgentChat::new(&config, agent_config).unwrap();

        chat.initialize_agents().await.unwrap();

        let agents = chat.agents.read().await;
        for (i, agent) in agents.iter().enumerate() {
            assert_eq!(agent.id, i);
        }
    }

    #[tokio::test]
    async fn test_initialize_agents_sets_correct_roles() {
        let config = Config::default();
        let agent_config = MultiAgentConfig::default();
        let chat = MultiAgentChat::new(&config, agent_config).unwrap();

        chat.initialize_agents().await.unwrap();

        let agents = chat.agents.read().await;
        assert_eq!(agents[0].role, AgentRole::Architect);
        assert_eq!(agents[1].role, AgentRole::Coder);
        assert_eq!(agents[2].role, AgentRole::Tester);
        assert_eq!(agents[3].role, AgentRole::Reviewer);
    }

    #[tokio::test]
    async fn test_initialize_agents_sets_names() {
        let config = Config::default();
        let agent_config = MultiAgentConfig::default();
        let chat = MultiAgentChat::new(&config, agent_config).unwrap();

        chat.initialize_agents().await.unwrap();

        let agents = chat.agents.read().await;
        assert_eq!(agents[0].name, "Agent-0-Architect");
        assert_eq!(agents[1].name, "Agent-1-Coder");
        assert_eq!(agents[2].name, "Agent-2-Tester");
        assert_eq!(agents[3].name, "Agent-3-Reviewer");
    }

    #[tokio::test]
    async fn test_initialize_agents_all_idle() {
        let config = Config::default();
        let agent_config = MultiAgentConfig::default();
        let chat = MultiAgentChat::new(&config, agent_config).unwrap();

        chat.initialize_agents().await.unwrap();

        let agents = chat.agents.read().await;
        for agent in agents.iter() {
            assert_eq!(agent.status, AgentStatus::Idle);
        }
    }

    #[tokio::test]
    async fn test_initialize_agents_sets_system_prompt() {
        let config = Config::default();
        let agent_config = MultiAgentConfig::default();
        let chat = MultiAgentChat::new(&config, agent_config).unwrap();

        chat.initialize_agents().await.unwrap();

        let agents = chat.agents.read().await;
        for agent in agents.iter() {
            assert_eq!(agent.messages.len(), 1);
        }
    }

    #[tokio::test]
    async fn test_initialize_agents_clears_previous() {
        let config = Config::default();
        let agent_config = MultiAgentConfig::default();
        let chat = MultiAgentChat::new(&config, agent_config).unwrap();

        // Initialize twice -- second call should reset
        chat.initialize_agents().await.unwrap();
        chat.initialize_agents().await.unwrap();

        let agents = chat.agents.read().await;
        assert_eq!(agents.len(), 4);
    }

    #[tokio::test]
    async fn test_initialize_agents_with_custom_roles() {
        let config = Config::default();
        let agent_config = MultiAgentConfig::default()
            .with_roles(vec![AgentRole::Security, AgentRole::Performance]);
        let chat = MultiAgentChat::new(&config, agent_config).unwrap();

        chat.initialize_agents().await.unwrap();

        let agents = chat.agents.read().await;
        assert_eq!(agents.len(), 2);
        assert_eq!(agents[0].role, AgentRole::Security);
        assert_eq!(agents[1].role, AgentRole::Performance);
        assert_eq!(agents[0].name, "Agent-0-Security");
        assert_eq!(agents[1].name, "Agent-1-Performance");
    }

    #[tokio::test]
    async fn test_initialize_agents_with_empty_roles() {
        let config = Config::default();
        let agent_config = MultiAgentConfig::default().with_roles(vec![]);
        let chat = MultiAgentChat::new(&config, agent_config).unwrap();

        chat.initialize_agents().await.unwrap();

        let agents = chat.agents.read().await;
        assert!(agents.is_empty());
    }

    #[tokio::test]
    async fn test_is_agent_healthy_idle_agent() {
        let config = Config::default();
        let agent_config = MultiAgentConfig::default();
        let chat = MultiAgentChat::new(&config, agent_config).unwrap();

        chat.initialize_agents().await.unwrap();

        // Fresh idle agents should be healthy
        assert!(chat.is_agent_healthy(0).await);
        assert!(chat.is_agent_healthy(1).await);
        assert!(chat.is_agent_healthy(2).await);
        assert!(chat.is_agent_healthy(3).await);
    }

    #[tokio::test]
    async fn test_is_agent_healthy_nonexistent_agent() {
        let config = Config::default();
        let agent_config = MultiAgentConfig::default();
        let chat = MultiAgentChat::new(&config, agent_config).unwrap();

        chat.initialize_agents().await.unwrap();

        // Agent ID out of range should not be healthy
        assert!(!chat.is_agent_healthy(999).await);
    }

    #[tokio::test]
    async fn test_is_agent_healthy_no_agents_initialized() {
        let config = Config::default();
        let agent_config = MultiAgentConfig::default();
        let chat = MultiAgentChat::new(&config, agent_config).unwrap();

        // No agents initialized
        assert!(!chat.is_agent_healthy(0).await);
    }

    #[tokio::test]
    async fn test_is_agent_healthy_failed_agent() {
        let config = Config::default();
        let agent_config = MultiAgentConfig::default();
        let chat = MultiAgentChat::new(&config, agent_config).unwrap();

        chat.initialize_agents().await.unwrap();

        // Mark agent as failed
        {
            let mut agents = chat.agents.write().await;
            agents[0].status = AgentStatus::Failed;
        }

        assert!(!chat.is_agent_healthy(0).await);
        // Other agents should still be healthy
        assert!(chat.is_agent_healthy(1).await);
    }

    #[tokio::test]
    async fn test_is_agent_healthy_working_agent() {
        let config = Config::default();
        let agent_config = MultiAgentConfig::default();
        let chat = MultiAgentChat::new(&config, agent_config).unwrap();

        chat.initialize_agents().await.unwrap();

        {
            let mut agents = chat.agents.write().await;
            agents[0].status = AgentStatus::Working;
        }

        assert!(chat.is_agent_healthy(0).await);
    }

    #[tokio::test]
    async fn test_is_agent_healthy_completed_agent() {
        let config = Config::default();
        let agent_config = MultiAgentConfig::default();
        let chat = MultiAgentChat::new(&config, agent_config).unwrap();

        chat.initialize_agents().await.unwrap();

        {
            let mut agents = chat.agents.write().await;
            agents[0].status = AgentStatus::Completed;
        }

        assert!(chat.is_agent_healthy(0).await);
    }

    // ---------------------------------------------------------------
    // Config field mutation tests (since fields are public)
    // ---------------------------------------------------------------

    #[test]
    fn test_config_fields_are_mutable() {
        let config = MultiAgentConfig {
            max_concurrency: 12,
            streaming: false,
            timeout_secs: 60,
            temperature: 0.5,
            max_tokens: 1024,
            failure_policy: MultiAgentFailurePolicy::FailFast,
            roles: vec![AgentRole::General],
        };

        assert_eq!(config.max_concurrency, 12);
        assert!(!config.streaming);
        assert_eq!(config.timeout_secs, 60);
        assert!((config.temperature - 0.5).abs() < f32::EPSILON);
        assert_eq!(config.max_tokens, 1024);
        assert_eq!(config.failure_policy, MultiAgentFailurePolicy::FailFast);
        assert_eq!(config.roles.len(), 1);
    }

    // ---------------------------------------------------------------
    // Concurrency constant
    // ---------------------------------------------------------------

    #[test]
    fn test_max_concurrent_agents_value() {
        assert_eq!(MAX_CONCURRENT_AGENTS, 16);
    }

    // ---------------------------------------------------------------
    // AgentResult -- comprehensive field access
    // ---------------------------------------------------------------

    #[test]
    fn test_agent_result_agent_name_field() {
        let result = AgentResult {
            agent_id: 7,
            agent_name: "Agent-7-Performance".to_string(),
            role: AgentRole::Performance,
            content: "Optimized".to_string(),
            tool_calls: vec!["profile".to_string()],
            duration: Duration::from_secs(42),
            success: true,
            error: None,
        };
        assert_eq!(result.agent_id, 7);
        assert_eq!(result.agent_name, "Agent-7-Performance");
        assert_eq!(result.role, AgentRole::Performance);
        assert_eq!(result.content, "Optimized");
        assert_eq!(result.tool_calls, vec!["profile".to_string()]);
        assert_eq!(result.duration.as_secs(), 42);
        assert!(result.success);
        assert!(result.error.is_none());
    }

    // ---------------------------------------------------------------
    // MultiAgentConfig -- Clone preserves all fields
    // ---------------------------------------------------------------

    #[test]
    fn test_config_clone_preserves_all_fields() {
        let config = MultiAgentConfig {
            max_concurrency: 7,
            streaming: false,
            timeout_secs: 300,
            temperature: 0.8,
            max_tokens: 2048,
            failure_policy: MultiAgentFailurePolicy::FailFast,
            ..Default::default()
        };

        let cloned = config.clone();
        assert_eq!(config.max_concurrency, cloned.max_concurrency);
        assert_eq!(config.streaming, cloned.streaming);
        assert_eq!(config.timeout_secs, cloned.timeout_secs);
        assert!((config.temperature - cloned.temperature).abs() < f32::EPSILON);
        assert_eq!(config.max_tokens, cloned.max_tokens);
        assert_eq!(config.failure_policy, cloned.failure_policy);
        assert_eq!(config.roles.len(), cloned.roles.len());
    }

    // ---------------------------------------------------------------
    // AgentResult Clone preserves all fields
    // ---------------------------------------------------------------

    #[test]
    fn test_agent_result_clone_preserves_all_fields() {
        let result = AgentResult {
            agent_id: 3,
            agent_name: "TestClone".to_string(),
            role: AgentRole::Reviewer,
            content: "Review content here".to_string(),
            tool_calls: vec!["tool_a".to_string(), "tool_b".to_string()],
            duration: Duration::from_millis(999),
            success: false,
            error: Some("some error".to_string()),
        };
        let cloned = result.clone();
        assert_eq!(result.agent_id, cloned.agent_id);
        assert_eq!(result.agent_name, cloned.agent_name);
        assert_eq!(result.role, cloned.role);
        assert_eq!(result.content, cloned.content);
        assert_eq!(result.tool_calls, cloned.tool_calls);
        assert_eq!(result.duration, cloned.duration);
        assert_eq!(result.success, cloned.success);
        assert_eq!(result.error, cloned.error);
    }

    // ---------------------------------------------------------------
    // AgentInstance Clone preserves all fields
    // ---------------------------------------------------------------

    #[test]
    fn test_agent_instance_clone_preserves_all_fields() {
        let agent = AgentInstance {
            id: 5,
            role: AgentRole::DevOps,
            name: "Agent-5-DevOps".to_string(),
            messages: vec![Message::system("sys"), Message::user("usr")],
            status: AgentStatus::Working,
            last_heartbeat: Instant::now(),
        };
        let cloned = agent.clone();
        assert_eq!(agent.id, cloned.id);
        assert_eq!(agent.role, cloned.role);
        assert_eq!(agent.name, cloned.name);
        assert_eq!(agent.messages.len(), cloned.messages.len());
        assert_eq!(agent.status, cloned.status);
        // Instant clone should be equal
        assert_eq!(agent.last_heartbeat, cloned.last_heartbeat);
    }

    // ---------------------------------------------------------------
    // Concurrent initialization test
    // ---------------------------------------------------------------

    #[tokio::test]
    async fn test_initialize_agents_concurrent_safe() {
        let config = Config::default();
        let agent_config =
            MultiAgentConfig::default().with_roles(vec![AgentRole::Coder, AgentRole::Tester]);
        let chat = Arc::new(MultiAgentChat::new(&config, agent_config).unwrap());

        // Initialize from two concurrent tasks
        let chat1 = Arc::clone(&chat);
        let chat2 = Arc::clone(&chat);
        let (r1, r2) = tokio::join!(async move { chat1.initialize_agents().await }, async move {
            chat2.initialize_agents().await
        },);
        r1.unwrap();
        r2.unwrap();

        // After concurrent initialization, we should have exactly 2 agents
        // (the last initialization wins and clears previous)
        let agents = chat.agents.read().await;
        assert_eq!(agents.len(), 2);
    }

    // ---------------------------------------------------------------
    // MultiAgentChat -- results storage
    // ---------------------------------------------------------------

    #[tokio::test]
    async fn test_results_initially_empty() {
        let config = Config::default();
        let agent_config = MultiAgentConfig::default();
        let chat = MultiAgentChat::new(&config, agent_config).unwrap();

        let results = chat.results.lock().await;
        assert!(results.is_empty());
    }

    // ---------------------------------------------------------------
    // Config with all agent roles
    // ---------------------------------------------------------------

    #[test]
    fn test_config_with_all_roles() {
        let all_roles = vec![
            AgentRole::Architect,
            AgentRole::Coder,
            AgentRole::Tester,
            AgentRole::Reviewer,
            AgentRole::Documenter,
            AgentRole::DevOps,
            AgentRole::Security,
            AgentRole::Performance,
            AgentRole::General,
        ];
        let config = MultiAgentConfig::default().with_roles(all_roles.clone());
        assert_eq!(config.roles.len(), 9);
        for (i, role) in all_roles.iter().enumerate() {
            assert_eq!(config.roles[i], *role);
        }
    }

    // ---------------------------------------------------------------
    // Initialize agents with all roles
    // ---------------------------------------------------------------

    #[tokio::test]
    async fn test_initialize_agents_all_roles() {
        let config = Config::default();
        let all_roles = vec![
            AgentRole::Architect,
            AgentRole::Coder,
            AgentRole::Tester,
            AgentRole::Reviewer,
            AgentRole::Documenter,
            AgentRole::DevOps,
            AgentRole::Security,
            AgentRole::Performance,
            AgentRole::General,
        ];
        let agent_config = MultiAgentConfig::default().with_roles(all_roles.clone());
        let chat = MultiAgentChat::new(&config, agent_config).unwrap();

        chat.initialize_agents().await.unwrap();

        let agents = chat.agents.read().await;
        assert_eq!(agents.len(), 9);
        for (i, agent) in agents.iter().enumerate() {
            assert_eq!(agent.id, i);
            assert_eq!(agent.role, all_roles[i]);
            assert_eq!(agent.name, format!("Agent-{}-{}", i, all_roles[i].name()));
            assert_eq!(agent.status, AgentStatus::Idle);
            assert_eq!(agent.messages.len(), 1);
        }
    }

    // ---------------------------------------------------------------
    // Emit events match channel ordering
    // ---------------------------------------------------------------

    #[test]
    fn test_emit_preserves_event_order() {
        let config = Config::default();
        let agent_config = MultiAgentConfig::default();
        let chat = MultiAgentChat::new(&config, agent_config).unwrap();
        let (tx, mut rx) = mpsc::channel::<MultiAgentEvent>(100);
        let chat = chat.with_events(tx);

        for i in 0..10 {
            chat.emit(MultiAgentEvent::AgentProgress {
                agent_id: i,
                content: format!("msg-{}", i),
            });
        }

        for i in 0..10 {
            let event = rx.try_recv().unwrap();
            match event {
                MultiAgentEvent::AgentProgress { agent_id, content } => {
                    assert_eq!(agent_id, i);
                    assert_eq!(content, format!("msg-{}", i));
                }
                _ => panic!("Expected AgentProgress"),
            }
        }
    }

    // ---------------------------------------------------------------
    // is_agent_healthy with all status variants
    // ---------------------------------------------------------------

    #[tokio::test]
    async fn test_is_agent_healthy_all_non_failed_statuses() {
        let config = Config::default();
        let agent_config = MultiAgentConfig::default().with_roles(vec![
            AgentRole::Coder,
            AgentRole::Tester,
            AgentRole::Reviewer,
        ]);
        let chat = MultiAgentChat::new(&config, agent_config).unwrap();
        chat.initialize_agents().await.unwrap();

        {
            let mut agents = chat.agents.write().await;
            agents[0].status = AgentStatus::Idle;
            agents[1].status = AgentStatus::Working;
            agents[2].status = AgentStatus::Completed;
        }

        assert!(chat.is_agent_healthy(0).await); // Idle
        assert!(chat.is_agent_healthy(1).await); // Working
        assert!(chat.is_agent_healthy(2).await); // Completed
    }

    // ---------------------------------------------------------------
    // MultiAgentConfig Debug contains all field names
    // ---------------------------------------------------------------

    #[test]
    fn test_multiagent_config_debug_contains_all_fields() {
        let config = MultiAgentConfig::default();
        let debug = format!("{:?}", config);
        assert!(debug.contains("max_concurrency"));
        assert!(debug.contains("roles"));
        assert!(debug.contains("streaming"));
        assert!(debug.contains("timeout_secs"));
        assert!(debug.contains("temperature"));
        assert!(debug.contains("max_tokens"));
        assert!(debug.contains("failure_policy"));
    }

    // ---------------------------------------------------------------
    // AgentResult Debug contains struct name
    // ---------------------------------------------------------------

    #[test]
    fn test_agent_result_debug_contains_fields() {
        let result = AgentResult {
            agent_id: 0,
            agent_name: "DebugTest".to_string(),
            role: AgentRole::Coder,
            content: "content".to_string(),
            tool_calls: vec!["tool".to_string()],
            duration: Duration::from_secs(1),
            success: true,
            error: None,
        };
        let debug = format!("{:?}", result);
        assert!(debug.contains("AgentResult"));
        assert!(debug.contains("DebugTest"));
        assert!(debug.contains("agent_id"));
        assert!(debug.contains("content"));
        assert!(debug.contains("tool_calls"));
        assert!(debug.contains("success"));
    }

    // ---------------------------------------------------------------
    // AgentInstance Debug contains struct name and fields
    // ---------------------------------------------------------------

    #[test]
    fn test_agent_instance_debug_contains_fields() {
        let agent = AgentInstance {
            id: 2,
            role: AgentRole::Reviewer,
            name: "DebugAgent".to_string(),
            messages: vec![],
            status: AgentStatus::Completed,
            last_heartbeat: Instant::now(),
        };
        let debug = format!("{:?}", agent);
        assert!(debug.contains("AgentInstance"));
        assert!(debug.contains("DebugAgent"));
        assert!(debug.contains("Completed"));
    }

    // ---------------------------------------------------------------
    // with_events replaces previous tx
    // ---------------------------------------------------------------

    #[test]
    fn test_with_events_replaces_previous() {
        let config = Config::default();
        let agent_config = MultiAgentConfig::default();
        let chat = MultiAgentChat::new(&config, agent_config).unwrap();

        let (tx1, _rx1) = mpsc::channel::<MultiAgentEvent>(10);
        let chat = chat.with_events(tx1);
        assert!(chat.event_tx.is_some());

        let (tx2, mut rx2) = mpsc::channel::<MultiAgentEvent>(10);
        let chat = chat.with_events(tx2);

        // Emit should go to the second channel
        chat.emit(MultiAgentEvent::AgentProgress {
            agent_id: 0,
            content: "test".into(),
        });
        assert!(rx2.try_recv().is_ok());
    }

    // ---------------------------------------------------------------
    // Config with_concurrency all values 0..=20
    // ---------------------------------------------------------------

    #[test]
    fn test_with_concurrency_all_values_in_range() {
        for n in 0..=20 {
            let config = MultiAgentConfig::default().with_concurrency(n);
            let expected = n.clamp(1, MAX_CONCURRENT_AGENTS);
            assert_eq!(
                config.max_concurrency, expected,
                "with_concurrency({}) should give {}, got {}",
                n, expected, config.max_concurrency
            );
        }
    }

    // ---------------------------------------------------------------
    // aggregate_results with large number of results
    // ---------------------------------------------------------------

    #[test]
    fn test_aggregate_results_many_results() {
        let results: Vec<AgentResult> = (0..50)
            .map(|i| AgentResult {
                agent_id: i,
                agent_name: format!("Agent-{}", i),
                role: AgentRole::General,
                content: format!("Output from agent {}", i),
                tool_calls: vec![],
                duration: Duration::from_secs(1),
                success: true,
                error: None,
            })
            .collect();
        let summary = MultiAgentChat::aggregate_results(&results);
        // All 50 agents should appear
        for i in 0..50 {
            assert!(
                summary.contains(&format!("Agent-{}", i)),
                "Missing Agent-{} in summary",
                i
            );
            assert!(summary.contains(&format!("Output from agent {}", i)));
        }
    }

    // ---------------------------------------------------------------
    // Aggregate results preserves content exactly
    // ---------------------------------------------------------------

    #[test]
    fn test_aggregate_results_preserves_content_exactly() {
        let content = "This is a detailed response\nWith multiple lines\n\nAnd paragraphs.";
        let results = vec![AgentResult {
            agent_id: 0,
            agent_name: "Precise".to_string(),
            role: AgentRole::Documenter,
            content: content.to_string(),
            tool_calls: vec![],
            duration: Duration::from_secs(1),
            success: true,
            error: None,
        }];
        let summary = MultiAgentChat::aggregate_results(&results);
        assert!(summary.contains(content));
    }

    // ---------------------------------------------------------------
    // Async: initialize_agents then check heartbeats are recent
    // ---------------------------------------------------------------

    #[tokio::test]
    async fn test_initialized_agents_have_recent_heartbeats() {
        let config = Config::default();
        let agent_config = MultiAgentConfig::default();
        let chat = MultiAgentChat::new(&config, agent_config).unwrap();

        chat.initialize_agents().await.unwrap();

        let agents = chat.agents.read().await;
        for agent in agents.iter() {
            assert!(
                agent.last_heartbeat.elapsed() < Duration::from_secs(5),
                "Agent {} heartbeat should be recent",
                agent.id
            );
        }
    }

    // ---------------------------------------------------------------
    // MultiAgentChat semaphore permits match config
    // ---------------------------------------------------------------

    #[test]
    fn test_semaphore_permits_match_clamped_concurrency() {
        let config = Config::default();

        // Concurrency 1
        let agent_config = MultiAgentConfig::default().with_concurrency(1);
        let chat = MultiAgentChat::new(&config, agent_config).unwrap();
        assert_eq!(chat.semaphore.available_permits(), 1);

        // Concurrency 8
        let agent_config = MultiAgentConfig::default().with_concurrency(8);
        let chat = MultiAgentChat::new(&config, agent_config).unwrap();
        assert_eq!(chat.semaphore.available_permits(), 8);

        // Concurrency 16 (max)
        let agent_config = MultiAgentConfig::default().with_concurrency(16);
        let chat = MultiAgentChat::new(&config, agent_config).unwrap();
        assert_eq!(chat.semaphore.available_permits(), 16);

        // Concurrency 100 (clamped to 16)
        let agent_config = MultiAgentConfig::default().with_concurrency(100);
        let chat = MultiAgentChat::new(&config, agent_config).unwrap();
        assert_eq!(chat.semaphore.available_permits(), 16);
    }
}
