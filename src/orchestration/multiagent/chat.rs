//! Multi-Agent Chat
//!
//! The main multi-agent chat orchestrator and execution logic.

use anyhow::{Context, Result};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, Mutex, RwLock, Semaphore};
use tokio::task::JoinSet;

use crate::api::types::Message;
use crate::api::{ApiClient, ThinkingMode};
use crate::config::Config;
use crate::tool_parser::parse_tool_calls;
use crate::tools::ToolRegistry;

use super::config::{MultiAgentConfig, MultiAgentFailurePolicy};
use super::types::{
    AgentInstance, AgentResult, AgentStatus, MultiAgentEvent, MAX_CONCURRENT_AGENTS,
};

/// Multi-agent chat orchestrator
///
/// NOTE: This struct uses `tokio::sync::RwLock` and `tokio::sync::Mutex`, which do NOT
/// poison on panic (unlike their `std::sync` counterparts). Therefore lock poisoning
/// recovery is not needed here.
pub struct MultiAgentChat {
    pub(super) config: MultiAgentConfig,
    pub(super) client: Arc<ApiClient>,
    pub(super) tools: Arc<ToolRegistry>,
    pub(super) semaphore: Arc<Semaphore>,
    pub(super) agents: Arc<RwLock<Vec<AgentInstance>>>,
    pub(super) results: Arc<Mutex<Vec<AgentResult>>>,
    pub(super) event_tx: Option<mpsc::Sender<MultiAgentEvent>>,
}

impl MultiAgentChat {
    /// Default heartbeat timeout: an agent is considered unhealthy if its
    /// last heartbeat is older than this duration.
    pub const HEARTBEAT_TIMEOUT: Duration = Duration::from_secs(300);

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
            let tools = Arc::clone(&self.tools);
            let semaphore = Arc::clone(&self.semaphore);
            let agents = Arc::clone(&self.agents);
            let results = Arc::clone(&self.results);
            let task = task.to_string();
            let timeout = Duration::from_secs(self.config.timeout_secs);
            let event_tx = self.event_tx.clone();
            let failure_policy = self.config.failure_policy;
            let cancelled = Arc::clone(&cancelled);
            // Per-agent overrides: build a dedicated ApiClient that uses the
            // temperature / max_tokens from MultiAgentConfig rather than the
            // base API config values.  Config is Clone, so this is cheap.
            let mut per_agent_config = self.client.config().clone();
            per_agent_config.temperature = self.config.temperature;
            per_agent_config.max_tokens = self.config.max_tokens;
            let per_agent_client = Arc::new(
                ApiClient::new(&per_agent_config)
                    .context("Failed to create per-agent API client")?,
            );

            join_set.spawn(async move {
                tokio::select! {
                    _ = cancelled.notified() => {
                        // Aborted by policy
                        Ok(())
                    }
                    res = Self::run_single_agent(
                        agent_id, task, per_agent_client, tools, semaphore, agents, results, timeout, event_tx,
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

        // Update agent status and heartbeat, and append the assistant
        // response back into the agent's message history so that
        // subsequent turns include the conversation context.
        {
            let mut agents = agents.write().await;
            if let Some(agent) = agents.get_mut(agent_id) {
                agent.status = if agent_result.success {
                    AgentStatus::Completed
                } else {
                    AgentStatus::Failed
                };
                agent.last_heartbeat = Instant::now();
                // Append the assistant response (or error text) so the
                // agent accumulates conversation history across turns.
                if agent_result.success {
                    agent
                        .messages
                        .push(Message::assistant(&agent_result.content));
                }
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
