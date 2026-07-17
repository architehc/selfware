//! Multi-Agent Chat
//!
//! The main multi-agent chat orchestrator and execution logic.
//!
//! Each agent in the fan-out makes exactly **one non-streaming chat
//! completion** — no tools, no ReAct loop. Results are aggregated verbatim.

use anyhow::{Context, Result};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, Mutex, RwLock, Semaphore};
use tokio::task::JoinSet;

use crate::api::types::{Message, Usage};
use crate::api::{ApiClient, ThinkingMode};
use crate::config::Config;

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

        let concurrency = agent_config.max_concurrency.clamp(1, MAX_CONCURRENT_AGENTS);

        Ok(Self {
            config: agent_config,
            client: Arc::new(client),
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

    /// Run a task across all agents concurrently.
    ///
    /// Each agent makes exactly one non-streaming chat completion (no tools,
    /// no agentic loop) and the results are returned in completion order.
    ///
    /// Sampling/timeout resolution per agent: `MultiAgentConfig` overrides
    /// win when set, otherwise the loaded [`Config`] values are inherited
    /// (`temperature`, `max_tokens`, and `agent.step_timeout_secs` for the
    /// per-call timeout).
    ///
    /// Spend guardrails: `--max-budget-tokens` / `--max-cost-usd` (merged by
    /// the CLI into `config.agent`) are enforced across the fan-out — see
    /// [`BudgetGuard`]. `--max-turns` (`agent.max_iterations`) has no meaning
    /// here because every agent makes exactly one call; it is deliberately
    /// ignored rather than silently half-applied.
    ///
    /// Returns an error immediately when a configured limit is already
    /// exhausted (zero budget) before any paid call is made. Otherwise the
    /// run completes with partial results: agents that were skipped to stay
    /// within budget appear as failed results whose error explains why.
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

        let base_config = self.client.config();

        // Resolve sampling/timeout: explicit MultiAgentConfig overrides win,
        // otherwise inherit the user's loaded Config (P1-1/P1-3).
        let resolved_max_tokens = self.config.max_tokens.unwrap_or(base_config.max_tokens);
        let timeout = Duration::from_secs(
            self.config
                .timeout_secs
                .unwrap_or(base_config.agent.step_timeout_secs),
        );

        // ── Spend guardrails (P0-3) ─────────────────────────────────────
        // Refuse to start a fan-out whose configured limits are already
        // exhausted; the per-agent gate then keeps cumulative spend within
        // the limits (see BudgetGuard).
        let limits = BudgetLimits {
            max_budget_tokens: base_config.agent.max_budget_tokens,
            max_cost_usd: base_config.agent.max_cost_usd,
        };
        if let Some(0) = limits.max_budget_tokens {
            anyhow::bail!(
                "multi-chat: --max-budget-tokens is 0; no agent calls can be made. \
                 Raise the budget or drop the flag."
            );
        }
        if let Some(max) = limits.max_cost_usd {
            anyhow::ensure!(
                max.is_finite() && max > 0.0,
                "multi-chat: --max-cost-usd must be a positive number (got {max}); \
                 no agent calls can be made."
            );
        }
        let budget = BudgetGuard::new(limits, resolved_max_tokens);

        // Build a dedicated per-agent client only when sampling overrides
        // are actually set; otherwise reuse the base client (which already
        // carries the user's temperature/max_tokens).
        let client = if self.config.temperature.is_some() || self.config.max_tokens.is_some() {
            let mut per_agent_config = base_config.clone();
            if let Some(t) = self.config.temperature {
                per_agent_config.temperature = t;
            }
            if let Some(mt) = self.config.max_tokens {
                per_agent_config.max_tokens = mt;
            }
            Arc::new(
                ApiClient::new(&per_agent_config)
                    .context("Failed to create per-agent API client")?,
            )
        } else {
            Arc::clone(&self.client)
        };

        // Shared cancellation state for FailFast policy
        let cancelled = Arc::new(tokio::sync::Notify::new());

        // Spawn concurrent agent tasks using JoinSet for structured cancellation
        let mut join_set = JoinSet::new();

        for agent_id in 0..agent_count {
            let semaphore = Arc::clone(&self.semaphore);
            let agents = Arc::clone(&self.agents);
            let results = Arc::clone(&self.results);
            let task = task.to_string();
            let event_tx = self.event_tx.clone();
            let failure_policy = self.config.failure_policy;
            let cancelled = Arc::clone(&cancelled);
            let client = Arc::clone(&client);
            let budget = budget.clone();

            join_set.spawn(async move {
                tokio::select! {
                    _ = cancelled.notified() => {
                        // Aborted by policy
                        Ok(())
                    }
                    res = Self::run_single_agent(
                        agent_id, task, client, budget, semaphore, agents, results, timeout, event_tx,
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

    /// Run a single agent's task: one non-streaming chat completion, no tools.
    #[allow(clippy::too_many_arguments)]
    async fn run_single_agent(
        agent_id: usize,
        task: String,
        client: Arc<ApiClient>,
        budget: BudgetGuard,
        semaphore: Arc<Semaphore>,
        agents: Arc<RwLock<Vec<AgentInstance>>>,
        results: Arc<Mutex<Vec<AgentResult>>>,
        timeout: Duration,
        event_tx: Option<mpsc::Sender<MultiAgentEvent>>,
    ) -> Result<()> {
        // Acquire semaphore permit
        let _permit = semaphore.acquire().await?;

        let start = Instant::now();

        // Budget gate (P0-3): check before doing any paid work. A skip is
        // recorded honestly but is NOT an error — it must not trip FailFast
        // and cancel sibling agents that are legitimately in flight.
        if let Err(reason) = budget.try_reserve() {
            let (agent_name, role) = {
                let agents = agents.read().await;
                match agents.get(agent_id) {
                    Some(a) => (a.name.clone(), a.role),
                    None => return Ok(()),
                }
            };
            tracing::info!("multi-chat: agent {} not launched: {}", agent_id, reason);
            if let Some(ref tx) = event_tx {
                let _ = tx.try_send(MultiAgentEvent::AgentFailed {
                    agent_id,
                    error: reason.clone(),
                });
            }
            let mut results = results.lock().await;
            results.push(AgentResult {
                agent_id,
                agent_name,
                role,
                content: String::new(),
                usage: None,
                duration: start.elapsed(),
                success: false,
                error: Some(reason),
            });
            return Ok(());
        }

        // Get agent info and update status + heartbeat
        let (agent_name, role, mut messages) = {
            let mut agents = agents.write().await;
            if let Some(agent) = agents.get_mut(agent_id) {
                agent.status = AgentStatus::Working;
                agent.last_heartbeat = Instant::now();
                (agent.name.clone(), agent.role, agent.messages.clone())
            } else {
                budget.settle(None);
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

        // Settle the budget reservation: drop the pessimistic estimate and
        // record actual provider-reported usage, when we got a response.
        // (A timed-out call may still have been billed by the provider, but
        // we only account what we can see.)
        budget.settle(match &result {
            Ok(Ok(response)) => Some(&response.usage),
            _ => None,
        });

        let duration = start.elapsed();

        let agent_result = match result {
            Ok(Ok(response)) => {
                let content = response
                    .choices
                    .first()
                    .map(|c| c.message.content.text().to_string())
                    .unwrap_or_default();

                AgentResult {
                    agent_id,
                    agent_name: agent_name.clone(),
                    role,
                    content,
                    usage: Some(response.usage),
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
                    usage: None,
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
                    usage: None,
                    duration,
                    success: false,
                    error: Some(error),
                }
            }
        };

        // Update agent status and heartbeat. On success, append BOTH the
        // user task and the assistant reply to the agent's history so the
        // next turn sees the full conversation with correct role
        // alternation (previously the user turn was lost, producing
        // `[system, assistant, user]`). On failure the history is left
        // untouched so it never ends on a dangling user message.
        {
            let mut agents = agents.write().await;
            if let Some(agent) = agents.get_mut(agent_id) {
                agent.status = if agent_result.success {
                    AgentStatus::Completed
                } else {
                    AgentStatus::Failed
                };
                agent.last_heartbeat = Instant::now();
                if agent_result.success {
                    agent.messages.push(Message::user(&task));
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

    /// Sum provider-reported usage across agent results.
    ///
    /// `cost` is `Some` only when at least one agent's provider reported a
    /// USD cost (e.g. OpenRouter's `usage.cost`); it stays `None` for
    /// providers that don't report cost at all.
    pub fn total_usage(results: &[AgentResult]) -> Usage {
        let mut total = Usage {
            prompt_tokens: 0,
            completion_tokens: 0,
            total_tokens: 0,
            cost: None,
        };
        for result in results {
            if let Some(usage) = &result.usage {
                total.prompt_tokens += usage.prompt_tokens;
                total.completion_tokens += usage.completion_tokens;
                total.total_tokens += usage.total_tokens;
                if let Some(cost) = usage.cost {
                    *total.cost.get_or_insert(0.0) += cost;
                }
            }
        }
        total
    }
}

/// Spend limits for one fan-out run, read from `config.agent` (where the CLI
/// merges `--max-budget-tokens` / `--max-cost-usd`).
#[derive(Debug, Clone, Copy, Default)]
struct BudgetLimits {
    max_budget_tokens: Option<usize>,
    max_cost_usd: Option<f64>,
}

/// Cumulative spend accounting shared by all agent tasks of one `run_task`.
///
/// Token enforcement is *pessimistic*: each in-flight call reserves the
/// per-call `max_tokens` cap when it starts and settles to the
/// provider-reported `usage.total_tokens` when it completes, so a
/// concurrent wave cannot overshoot the token budget. Cost enforcement uses
/// actual provider-reported `usage.cost`, which is only known after a call
/// completes — a wave already in flight is allowed to finish, and later
/// launches are then blocked.
///
/// Uses `std::sync::Mutex` (not tokio): critical sections are a few integer
/// ops and are never held across an `.await`.
#[derive(Debug, Default)]
struct BudgetTracker {
    /// Sum of `usage.total_tokens` reported by completed calls.
    actual_tokens: usize,
    /// Sum of provider-reported `usage.cost` from completed calls.
    actual_cost: f64,
    /// Pessimistic reservations held by in-flight calls.
    reserved_tokens: usize,
}

/// Per-run spend guard passed to every agent task.
#[derive(Debug, Clone)]
struct BudgetGuard {
    tracker: Arc<std::sync::Mutex<BudgetTracker>>,
    limits: BudgetLimits,
    /// Pessimistic per-call token estimate (the resolved `max_tokens` cap).
    estimate: usize,
}

impl BudgetGuard {
    fn new(limits: BudgetLimits, estimate: usize) -> Self {
        Self {
            tracker: Arc::new(std::sync::Mutex::new(BudgetTracker::default())),
            limits,
            estimate,
        }
    }

    /// Try to reserve budget for one new call. Returns a human-readable
    /// reason when launching the call would exceed a configured limit.
    fn try_reserve(&self) -> Result<(), String> {
        let mut tracker = self.tracker.lock().unwrap();
        if let Some(max) = self.limits.max_budget_tokens {
            let committed = tracker.actual_tokens + tracker.reserved_tokens;
            if committed + self.estimate > max {
                return Err(format!(
                    "skipped to stay within --max-budget-tokens={max}: \
                     {committed} tokens used/reserved + ~{} estimated for this call",
                    self.estimate
                ));
            }
        }
        if let Some(max) = self.limits.max_cost_usd {
            if tracker.actual_cost >= max {
                return Err(format!(
                    "skipped to stay within --max-cost-usd=${max}: \
                     ${:.6} already spent",
                    tracker.actual_cost
                ));
            }
        }
        tracker.reserved_tokens += self.estimate;
        Ok(())
    }

    /// Settle a finished (or abandoned) call: release its reservation and
    /// record actual provider-reported usage, when available.
    fn settle(&self, usage: Option<&Usage>) {
        let mut tracker = self.tracker.lock().unwrap();
        tracker.reserved_tokens = tracker.reserved_tokens.saturating_sub(self.estimate);
        if let Some(usage) = usage {
            tracker.actual_tokens += usage.total_tokens;
            if let Some(cost) = usage.cost {
                tracker.actual_cost += cost;
            }
        }
    }
}
