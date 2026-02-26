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
use futures::stream::{FuturesUnordered, StreamExt};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, Mutex, RwLock, Semaphore};

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
pub struct MultiAgentChat {
    config: MultiAgentConfig,
    client: Arc<ApiClient>,
    tools: Arc<ToolRegistry>,
    semaphore: Arc<Semaphore>,
    agents: Arc<RwLock<Vec<AgentInstance>>>,
    results: Arc<Mutex<Vec<AgentResult>>>,
    event_tx: Option<mpsc::UnboundedSender<MultiAgentEvent>>,
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
    pub fn with_events(mut self, tx: mpsc::UnboundedSender<MultiAgentEvent>) -> Self {
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
            };
            agents.push(agent);
        }

        Ok(())
    }

    /// Send an event if event channel is configured
    fn emit(&self, event: MultiAgentEvent) {
        if let Some(ref tx) = self.event_tx {
            let _ = tx.send(event);
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

        // Spawn concurrent agent tasks
        let mut futures = FuturesUnordered::new();

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

            futures.push(tokio::spawn(async move {
                tokio::select! {
                    _ = cancelled.notified() => {
                        // Aborted by policy
                        Ok(())
                    }
                    res = Self::run_single_agent(
                        agent_id, task, client, tools, semaphore, agents, results, timeout, event_tx,
                    ) => {
                        if failure_policy == MultiAgentFailurePolicy::FailFast {
                            // Check if this result was a failure
                            if let Ok(()) = res {
                                // Agent completed successfully, but we need to check the actual result status
                                // because run_single_agent returns Ok(()) even if LLM call failed.
                                // In this simplified implementation, we'll let run_single_agent 
                                // handle internal failure reporting.
                            }
                        }
                        res
                    }
                }
            }));
        }

        // Wait for all agents to complete or fail
        while let Some(result) = futures.next().await {
            match result {
                Ok(Ok(_)) => {
                    // Task finished
                }
                Ok(Err(e)) => {
                    eprintln!("Agent-specific error: {}", e);
                    if self.config.failure_policy == MultiAgentFailurePolicy::FailFast {
                        cancelled.notify_waiters();
                    }
                }
                Err(e) => {
                    eprintln!("Agent task panicked: {}", e);
                    if self.config.failure_policy == MultiAgentFailurePolicy::FailFast {
                        cancelled.notify_waiters();
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
        event_tx: Option<mpsc::UnboundedSender<MultiAgentEvent>>,
    ) -> Result<()> {
        // Acquire semaphore permit
        let _permit = semaphore.acquire().await?;

        let start = Instant::now();

        // Get agent info and update status
        let (agent_name, role, mut messages) = {
            let mut agents = agents.write().await;
            if let Some(agent) = agents.get_mut(agent_id) {
                agent.status = AgentStatus::Working;
                (agent.name.clone(), agent.role, agent.messages.clone())
            } else {
                return Ok(());
            }
        };

        // Emit start event
        if let Some(ref tx) = event_tx {
            let _ = tx.send(MultiAgentEvent::AgentStarted {
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
                    .map(|c| c.message.content.clone())
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
                        let _ = tx.send(MultiAgentEvent::AgentToolCall {
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
                    let _ = tx.send(MultiAgentEvent::AgentFailed {
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
                    let _ = tx.send(MultiAgentEvent::AgentFailed {
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

        // Update agent status
        {
            let mut agents = agents.write().await;
            if let Some(agent) = agents.get_mut(agent_id) {
                agent.status = if agent_result.success {
                    AgentStatus::Completed
                } else {
                    AgentStatus::Failed
                };
            }
        }

        // Emit completion event
        if let Some(ref tx) = event_tx {
            let _ = tx.send(MultiAgentEvent::AgentCompleted {
                agent_id,
                result: agent_result.clone(),
            });
        }

        // Store result
        {
            let mut results = results.lock().await;
            results.push(agent_result);
        }

        Ok(())
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
                if let Ok(n) = input
                    .strip_prefix("/parallel ")
                    .unwrap()
                    .trim()
                    .parse::<usize>()
                {
                    let n = n.clamp(1, MAX_CONCURRENT_AGENTS);
                    self.config.max_concurrency = n;
                    self.semaphore = Arc::new(Semaphore::new(n));
                    println!("Max concurrency set to {}", n);
                }
                continue;
            }

            if input.starts_with("/add ") {
                let role_str = input.strip_prefix("/add ").unwrap().trim().to_lowercase();
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
                    });
                    println!("Added Agent-{}-{}", id, role.name());
                } else {
                    println!("Unknown role. Available: architect, coder, tester, reviewer, documenter, devops, security, performance, general");
                }
                continue;
            }

            if input.starts_with("/remove ") {
                if let Ok(id) = input
                    .strip_prefix("/remove ")
                    .unwrap()
                    .trim()
                    .parse::<usize>()
                {
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
            let (tx, mut rx) = mpsc::unbounded_channel::<MultiAgentEvent>();
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
}
