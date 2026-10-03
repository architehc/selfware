//! Agent Workflows
//!
//! YAML-defined templates for common development workflows.
//! Supports TDD, Debug, Refactor, Review, and custom workflows.
//!
//! # Execution Modes
//!
//! The workflow executor supports two modes:
//!
//! - **Live mode** (default): Shell commands are executed via `sh -c`, with stdout/stderr
//!   captured. Tool and LLM steps require injected handlers.
//!
//! - **Dry-run mode**: All steps log their intended actions without executing. Useful for
//!   workflow validation and testing.
//!
//! # Features
//!
//! - Declarative workflow definitions
//! - Step-by-step execution with real shell commands
//! - Conditional branching
//! - Conditional loops (`type: until` — "fix, then re-test, until green", bounded by a
//!   required `max_iterations`)
//! - Variable substitution
//! - Tool integration (via handler injection)
//! - Progress tracking

use crate::observability::telemetry::{
    add_tokens_processed, record_workflow_llm_call, record_workflow_run,
};
use crate::tools::process_guard::GroupedOutputExt;
use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use tokio::process::Command;
mod checkpoint;
mod templates;
pub use checkpoint::{
    new_run_id, step_fingerprint, validate_run_id, CheckpointStore, WorkflowCheckpoint,
    WorkflowRun, CHECKPOINT_FORMAT_VERSION,
};
#[cfg(test)]
mod test_budget;
#[cfg(test)]
mod test_checkpoint;
#[cfg(test)]
mod test_context;
#[cfg(test)]
mod test_execution;
#[cfg(test)]
mod test_models;
#[cfg(test)]
mod test_templates;
#[cfg(test)]
mod test_until;

/// Workflow execution status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum WorkflowStatus {
    /// Not yet started
    #[default]
    Pending,
    /// Currently running
    Running,
    /// Completed successfully
    Completed,
    /// Failed with error
    Failed,
    /// Paused (reserved: no step currently produces it; failed or stopped
    /// runs are resumed from their checkpoint, see `WorkflowRun`)
    Paused,
    /// Cancelled by user
    Cancelled,
}

/// Step status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum StepStatus {
    #[default]
    Pending,
    Running,
    Completed,
    Failed,
    Skipped,
}

/// Variable type in workflows
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(untagged)]
pub enum VarValue {
    String(String),
    Number(f64),
    Boolean(bool),
    List(Vec<VarValue>),
    Map(HashMap<String, VarValue>),
    #[default]
    Null,
}

impl VarValue {
    /// Get as string
    pub fn as_string(&self) -> Option<String> {
        match self {
            VarValue::String(s) => Some(s.clone()),
            VarValue::Number(n) => Some(n.to_string()),
            VarValue::Boolean(b) => Some(b.to_string()),
            _ => None,
        }
    }

    /// Get as bool
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            VarValue::Boolean(b) => Some(*b),
            VarValue::String(s) => Some(!s.is_empty()),
            VarValue::Number(n) => Some(*n != 0.0),
            VarValue::Null => Some(false),
            _ => None,
        }
    }
}

impl From<&str> for VarValue {
    fn from(s: &str) -> Self {
        VarValue::String(s.to_string())
    }
}

impl From<String> for VarValue {
    fn from(s: String) -> Self {
        VarValue::String(s)
    }
}

impl From<bool> for VarValue {
    fn from(b: bool) -> Self {
        VarValue::Boolean(b)
    }
}

impl From<i32> for VarValue {
    fn from(n: i32) -> Self {
        VarValue::Number(n as f64)
    }
}

/// Workflow step type
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum StepType {
    /// Execute a tool
    Tool {
        /// Tool to execute. Serialized as `tool` because the flattened
        /// `WorkflowStep.name` already claims the `name` key — without the
        /// rename the two collide and the step can never deserialize.
        #[serde(rename = "tool")]
        name: String,
        #[serde(default)]
        args: HashMap<String, String>,
    },
    /// Run a shell command
    Shell {
        command: String,
        #[serde(default)]
        working_dir: Option<String>,
    },
    /// Ask the LLM to perform a task
    Llm {
        prompt: String,
        #[serde(default)]
        context: Vec<String>,
    },
    /// Prompt user for input
    Input {
        prompt: String,
        #[serde(default)]
        variable: String,
        #[serde(default)]
        default: Option<String>,
    },
    /// Conditional step
    Condition {
        #[serde(rename = "if")]
        condition: String,
        #[serde(rename = "then")]
        then_steps: Vec<String>,
        #[serde(rename = "else")]
        else_steps: Option<Vec<String>>,
    },
    /// Loop over items
    Loop {
        #[serde(rename = "for")]
        variable: String,
        #[serde(rename = "in")]
        items: String,
        #[serde(rename = "do")]
        do_steps: Vec<String>,
    },
    /// Set a variable
    SetVar {
        /// Variable to set. Serialized as `var` (see `Tool` above).
        #[serde(rename = "var")]
        name: String,
        value: String,
    },
    /// Log a message
    Log {
        message: String,
        #[serde(default)]
        level: LogLevel,
    },
    /// Pause for user confirmation
    Pause { message: String },
    /// Call another workflow
    SubWorkflow {
        /// Name of the sub-workflow to execute
        #[serde(rename = "workflow")]
        workflow_name: String,
        #[serde(default)]
        inputs: HashMap<String, String>,
    },
    /// Conditional loop: run `do` steps, then evaluate `until`; repeat until
    /// the condition is true or `max_iterations` passes have run.
    ///
    /// This is the "fix, then re-test, until green" shape: failures of body
    /// steps do not abort the loop (a failing test is what the next pass
    /// fixes); the condition decides. After the loop, each body step's
    /// result is the one from the LAST pass, so a required body step that
    /// still failed on the final pass fails the workflow (honest status).
    ///
    /// `max_iterations` is required (no unbounded loops) and clamped to
    /// [`MAX_UNTIL_ITERATIONS`]. Exhausting it fails the step with an
    /// [`UntilExhausted`] error unless `on_exhausted: continue`.
    ///
    /// The step's `timeout_secs` bounds the WHOLE loop; when unset the
    /// default is the per-step default (300s) times the effective
    /// iteration cap, so each pass gets the budget a plain step gets.
    Until {
        #[serde(rename = "do")]
        do_steps: Vec<String>,
        /// Condition expression (same evaluator as `condition` steps:
        /// `success(id)`, `failed(id)`, `defined(var)`, `a == b`, ...).
        #[serde(rename = "until")]
        condition: String,
        /// Maximum number of passes (required, clamped to
        /// [`MAX_UNTIL_ITERATIONS`], must be >= 1).
        max_iterations: u32,
        /// What exhausting `max_iterations` means: `fail` (default) or
        /// `continue`.
        #[serde(default)]
        on_exhausted: UntilExhausted,
    },
    /// Guardrail enforcement step
    Guardrail {
        /// Guardrail name. Serialized as `guardrail` (see `Tool` above).
        #[serde(rename = "guardrail")]
        name: String,
        /// Condition expression to evaluate
        condition: String,
        /// Action on violation: "block", "warn", "log", "alert"
        on_violation: String,
        /// Severity level: "info", "low", "medium", "high", "critical"
        #[serde(default)]
        severity: String,
        /// Description of what this guardrail checks. Serialized as
        /// `guardrail_description` because the flattened
        /// `WorkflowStep.description` already claims the `description` key.
        #[serde(default, rename = "guardrail_description")]
        description: String,
    },
}

/// Hard ceiling on the passes an [`StepType::Until`] loop may run,
/// whatever its `max_iterations` says.
pub const MAX_UNTIL_ITERATIONS: u32 = 100;

/// Default per-step timeout when a step sets no `timeout_secs`.
const DEFAULT_STEP_TIMEOUT_SECS: u64 = 300;

/// What an [`StepType::Until`] loop does when `max_iterations` passes ran
/// without the condition becoming true.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum UntilExhausted {
    /// Fail the step with an [`UntilExhaustedError`] naming the cap.
    #[default]
    Fail,
    /// Log a warning and let the workflow continue.
    Continue,
}

/// Typed failure of an [`StepType::Until`] loop that ran out of passes.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error(
    "until loop exhausted max_iterations={max_iterations} without condition '{condition}' becoming true"
)]
pub struct UntilExhaustedError {
    /// The effective (clamped) iteration cap that was hit.
    pub max_iterations: u32,
    /// The condition expression as written.
    pub condition: String,
}

/// Effective iteration cap of an until loop: `max_iterations` clamped to
/// [`MAX_UNTIL_ITERATIONS`]; `None` when it is 0 (a definition error).
fn effective_until_iterations(max_iterations: u32) -> Option<u32> {
    (max_iterations >= 1).then(|| max_iterations.min(MAX_UNTIL_ITERATIONS))
}

/// The timeout a step runs under when it sets no `timeout_secs`.
fn default_step_timeout(step_type: &StepType) -> Duration {
    match step_type {
        StepType::Until { max_iterations, .. } => Duration::from_secs(
            DEFAULT_STEP_TIMEOUT_SECS
                * u64::from(effective_until_iterations(*max_iterations).unwrap_or(1)),
        ),
        _ => Duration::from_secs(DEFAULT_STEP_TIMEOUT_SECS),
    }
}

/// Log level
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Debug,
    #[default]
    Info,
    Warn,
    Error,
}

/// A single step in a workflow
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowStep {
    /// Step identifier
    pub id: String,
    /// Human-readable name
    pub name: String,
    /// Description
    #[serde(default)]
    pub description: String,
    /// Step type and action
    #[serde(flatten)]
    pub step_type: StepType,
    /// Whether this step is required (workflow fails if step fails)
    #[serde(default = "default_true")]
    pub required: bool,
    /// Retry configuration
    #[serde(default)]
    pub retry: RetryConfig,
    /// Timeout in seconds
    #[serde(default)]
    pub timeout_secs: Option<u64>,
    /// Dependencies (step IDs that must complete first)
    #[serde(default)]
    pub depends_on: Vec<String>,
}

fn default_true() -> bool {
    true
}

/// Retry configuration
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RetryConfig {
    /// Maximum number of retries
    #[serde(default)]
    pub max_attempts: u32,
    /// Delay between retries in seconds
    #[serde(default)]
    pub delay_secs: u64,
    /// Whether to use exponential backoff
    #[serde(default)]
    pub exponential: bool,
}

/// Workflow definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Workflow {
    /// Workflow name
    pub name: String,
    /// Description
    #[serde(default)]
    pub description: String,
    /// Version
    #[serde(default = "default_version")]
    pub version: String,
    /// Author
    #[serde(default)]
    pub author: String,
    /// Category/type
    #[serde(default)]
    pub category: String,
    /// Input parameters
    #[serde(default)]
    pub inputs: Vec<WorkflowInput>,
    /// Output definitions
    #[serde(default)]
    pub outputs: Vec<WorkflowOutput>,
    /// Steps in the workflow
    pub steps: Vec<WorkflowStep>,
    /// Tags for discovery
    #[serde(default)]
    pub tags: Vec<String>,
}

fn default_version() -> String {
    "1.0.0".to_string()
}

/// Workflow input parameter
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowInput {
    /// Parameter name
    pub name: String,
    /// Description
    #[serde(default)]
    pub description: String,
    /// Whether required
    #[serde(default)]
    pub required: bool,
    /// Default value
    #[serde(default)]
    pub default: Option<VarValue>,
    /// Type hint
    #[serde(default = "default_string_type")]
    pub param_type: String,
}

fn default_string_type() -> String {
    "string".to_string()
}

/// Workflow output
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowOutput {
    /// Output name
    pub name: String,
    /// Description
    #[serde(default)]
    pub description: String,
    /// Variable to use as output
    pub from: String,
}

/// Step execution result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepResult {
    /// Step ID
    pub step_id: String,
    /// Status
    pub status: StepStatus,
    /// Output value
    pub output: Option<VarValue>,
    /// Error message if failed
    pub error: Option<String>,
    /// Duration in milliseconds
    pub duration_ms: u64,
    /// Retry count
    pub retry_count: u32,
}

/// Token usage reported for a workflow LLM call.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct LlmTokenUsage {
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub total_tokens: u64,
}

/// Rich LLM step output used by the workflow executor.
#[derive(Debug, Clone, Default)]
pub struct LlmCallOutput {
    pub content: String,
    pub usage: Option<LlmTokenUsage>,
    pub model: Option<String>,
    /// Provider-reported cost of this call in USD (e.g. OpenRouter's
    /// `usage.cost`). `None` when the provider reported none — never an
    /// estimate from token counts (AGENTS.md rules 3 and 4).
    pub cost_usd: Option<f64>,
}

impl LlmCallOutput {
    pub fn text(content: impl Into<String>) -> Self {
        Self {
            content: content.into(),
            usage: None,
            model: None,
            cost_usd: None,
        }
    }

    pub fn with_usage(mut self, usage: LlmTokenUsage) -> Self {
        self.usage = Some(usage);
        self
    }

    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = Some(model.into());
        self
    }

    /// Attach the provider-reported cost; `None` leaves the call uncosted.
    pub fn with_reported_cost(mut self, cost_usd: Option<f64>) -> Self {
        self.cost_usd = cost_usd;
        self
    }
}

impl From<String> for LlmCallOutput {
    fn from(content: String) -> Self {
        Self::text(content)
    }
}

impl From<&str> for LlmCallOutput {
    fn from(content: &str) -> Self {
        Self::text(content)
    }
}

/// Aggregated telemetry for a workflow execution.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct WorkflowTelemetry {
    pub llm_calls: u64,
    pub llm_latency_ms: u64,
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub total_tokens: u64,
    /// Sum of provider-reported LLM cost in USD; `None` when no call
    /// reported a cost. Incomplete when `uncosted_llm_calls > 0`.
    #[serde(default)]
    pub cost_usd: Option<f64>,
    /// LLM calls whose provider reported no cost: `cost_usd` does not
    /// cover them, and nothing is estimated in their place.
    #[serde(default)]
    pub uncosted_llm_calls: u64,
    pub completed_steps: u64,
    pub failed_steps: u64,
    pub skipped_steps: u64,
    /// LLM calls whose handler reported no token usage. Their tokens are
    /// NOT in the totals above, so a `max_tokens` budget cannot see them;
    /// this count keeps that gap visible instead of silently under-billing.
    #[serde(default)]
    pub unmetered_llm_calls: u64,
}

impl WorkflowTelemetry {
    /// Measured tokens: the reported total, or prompt + completion when a
    /// provider left the total at 0.
    pub fn measured_tokens(&self) -> u64 {
        self.total_tokens
            .max(self.prompt_tokens.saturating_add(self.completion_tokens))
    }

    /// Add one call's provider-reported cost; `None` is counted as an
    /// uncosted call, never priced.
    fn add_reported_cost(&mut self, cost: Option<f64>) {
        match cost {
            Some(c) => self.cost_usd = Some(self.cost_usd.unwrap_or(0.0) + c),
            None => self.uncosted_llm_calls += 1,
        }
    }

    /// `cost $X` / `known cost $X (incomplete …)` / `cost not tracked …`:
    /// the same wording as the agent's `/cost` and run summary.
    pub fn cost_phrase(&self) -> String {
        crate::agent::session_usage::cost_phrase(
            self.cost_usd,
            self.uncosted_llm_calls == 0,
            usize::try_from(self.uncosted_llm_calls).unwrap_or(usize::MAX),
        )
    }

    /// Fold a sub-workflow's LLM usage into this (parent) telemetry so a
    /// parent budget and report cover the tokens its sub-workflows spent.
    /// Step counts are not merged: they are recomputed from the parent's
    /// own step results.
    fn absorb_llm_usage(&mut self, other: &WorkflowTelemetry) {
        self.llm_calls += other.llm_calls;
        self.llm_latency_ms += other.llm_latency_ms;
        self.prompt_tokens += other.prompt_tokens;
        self.completion_tokens += other.completion_tokens;
        self.total_tokens += other.total_tokens;
        if let Some(c) = other.cost_usd {
            self.cost_usd = Some(self.cost_usd.unwrap_or(0.0) + c);
        }
        self.uncosted_llm_calls += other.uncosted_llm_calls;
        self.unmetered_llm_calls += other.unmetered_llm_calls;
    }
}

/// Workflow-level resource budget, checked before every step the executor
/// starts: between top-level steps, between `until` passes, and before each
/// body step of a `loop` iteration, an `until` pass and a condition branch.
///
/// Declared at the top level of a workflow YAML (`max_wall_secs:`,
/// `max_tokens:`) or an SWL workflow definition. Tokens are the MEASURED
/// usage the LLM handler reported (`WorkflowTelemetry::measured_tokens`,
/// AGENTS.md §4) — calls that reported no usage are counted in
/// `unmetered_llm_calls` and named in the stop reason, never guessed.
/// The check runs between steps, so the step that crosses the limit can
/// overshoot it; the next step then does not start. Because loop, `until`
/// and condition bodies are checked per body step, the overrun is at most
/// one *leaf* step (a body step, or a whole sub-workflow call — a
/// sub-workflow's tokens reach the parent when it returns), never a whole
/// 1000-item loop (`formal/WorkflowBounds.lean` W3 over the flattened step
/// sequence).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkflowBudget {
    /// Wall-clock limit for the run, in seconds.
    #[serde(default)]
    pub max_wall_secs: Option<u64>,
    /// Measured-token limit for the run (all LLM steps, sub-workflows
    /// included).
    #[serde(default)]
    pub max_tokens: Option<u64>,
}

impl WorkflowBudget {
    /// True when no limit is set.
    pub fn is_unbounded(&self) -> bool {
        self.max_wall_secs.is_none() && self.max_tokens.is_none()
    }

    /// The stop reason if the run has used up this budget.
    pub fn exceeded(
        &self,
        elapsed_ms: u64,
        telemetry: &WorkflowTelemetry,
    ) -> Option<WorkflowStopReason> {
        if let Some(max_wall_secs) = self.max_wall_secs {
            if elapsed_ms >= max_wall_secs.saturating_mul(1000) {
                return Some(WorkflowStopReason::WallClockBudget {
                    max_wall_secs,
                    elapsed_ms,
                });
            }
        }
        if let Some(max_tokens) = self.max_tokens {
            let used_tokens = telemetry.measured_tokens();
            if used_tokens >= max_tokens {
                return Some(WorkflowStopReason::TokenBudget {
                    max_tokens,
                    used_tokens,
                    unmetered_llm_calls: telemetry.unmetered_llm_calls,
                });
            }
        }
        None
    }
}

/// Why a workflow stopped before running all of its steps.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, thiserror::Error)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum WorkflowStopReason {
    /// `max_wall_secs` was reached.
    #[error("wall-clock budget exhausted: {elapsed_ms}ms elapsed, max_wall_secs={max_wall_secs}")]
    WallClockBudget {
        /// The configured limit.
        max_wall_secs: u64,
        /// Elapsed run time when the check fired.
        elapsed_ms: u64,
    },
    /// `max_tokens` was reached.
    #[error(
        "token budget exhausted: {used_tokens} measured tokens used, max_tokens={max_tokens} ({unmetered_llm_calls} LLM call(s) reported no usage and are not counted)"
    )]
    TokenBudget {
        /// The configured limit.
        max_tokens: u64,
        /// Measured tokens used when the check fired.
        used_tokens: u64,
        /// LLM calls whose usage was unknown (not in `used_tokens`).
        unmetered_llm_calls: u64,
    },
}

/// Maximum recursion depth for nested step execution
const MAX_RECURSION_DEPTH: usize = 10;

/// Maximum number of workflow log entries before oldest entries are evicted.
const MAX_WORKFLOW_LOG_ENTRIES: usize = 1000;

/// Maximum number of items a single Loop step may iterate over.
pub const MAX_LOOP_ITEMS: usize = 1000;

/// Maximum number of step executions (every step run through the retry
/// runner, including loop iterations, condition branches and sub-workflow
/// steps) in one `execute` call. Bounds nested loops, whose item counts
/// multiply.
pub const MAX_STEP_EXECUTIONS: u64 = 10_000;

/// Typed workflow resource-limit errors (downcast from `anyhow::Error`).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum WorkflowLimitError {
    /// A Loop step's item list exceeded [`MAX_LOOP_ITEMS`].
    #[error(
        "loop over '{variable}' has {items} items, exceeding the limit of {limit} \
         (MAX_LOOP_ITEMS); split the list across several loop steps or workflows"
    )]
    LoopTooManyItems {
        variable: String,
        items: usize,
        limit: usize,
    },
    /// The run exceeded [`MAX_STEP_EXECUTIONS`] step executions.
    #[error(
        "workflow run exceeded {limit} step executions (MAX_STEP_EXECUTIONS, counted \
         across loops, condition branches and sub-workflows); reduce loop sizes or \
         split the run into several workflows"
    )]
    StepExecutionLimit { limit: u64 },
}

/// Workflow execution context
#[derive(Debug, Clone)]
pub struct WorkflowContext {
    /// Workflow name for telemetry labeling
    pub workflow_name: Option<String>,
    /// Variables
    pub variables: HashMap<String, VarValue>,
    /// Working directory
    pub working_dir: PathBuf,
    /// Step results
    pub step_results: HashMap<String, StepResult>,
    /// Current step index
    pub current_step: usize,
    /// Workflow status
    pub status: WorkflowStatus,
    /// Start time
    pub started_at: Option<Instant>,
    /// Log messages
    pub logs: VecDeque<LogEntry>,
    /// Current recursion depth for nested steps
    pub recursion_depth: usize,
    /// Step IDs currently being executed (for cycle detection)
    pub executing_steps: Vec<String>,
    /// Step IDs executed inline by control-flow (condition/loop) - skip in top-level pass
    pub control_flow_managed_steps: std::collections::HashSet<String>,
    /// Workflow call stack for cycle detection in sub-workflows
    pub workflow_call_stack: Vec<String>,
    /// Aggregated workflow telemetry
    pub telemetry: WorkflowTelemetry,
    /// Step executions so far in this run, shared with sub-workflows so
    /// [`MAX_STEP_EXECUTIONS`] bounds the whole run.
    pub step_executions: std::sync::Arc<std::sync::atomic::AtomicU64>,
    /// Resource budget of the running workflow
    pub budget: WorkflowBudget,
    /// Set when the run stopped early on a budget
    pub stop_reason: Option<WorkflowStopReason>,
}

/// Log entry
#[derive(Debug, Clone)]
pub struct LogEntry {
    pub timestamp: u64,
    pub level: LogLevel,
    pub message: String,
    pub step_id: Option<String>,
}

impl WorkflowContext {
    /// Create new context
    pub fn new(working_dir: impl Into<PathBuf>) -> Self {
        Self {
            workflow_name: None,
            variables: HashMap::new(),
            working_dir: working_dir.into(),
            step_results: HashMap::new(),
            current_step: 0,
            status: WorkflowStatus::Pending,
            started_at: None,
            logs: VecDeque::new(),
            recursion_depth: 0,
            executing_steps: Vec::new(),
            control_flow_managed_steps: std::collections::HashSet::new(),
            workflow_call_stack: Vec::new(),
            telemetry: WorkflowTelemetry::default(),
            step_executions: std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0)),
            budget: WorkflowBudget::default(),
            stop_reason: None,
        }
    }

    /// Charge one step execution against [`MAX_STEP_EXECUTIONS`].
    fn charge_step_execution(&self) -> Result<(), WorkflowLimitError> {
        let prev = self
            .step_executions
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        if prev >= MAX_STEP_EXECUTIONS {
            Err(WorkflowLimitError::StepExecutionLimit {
                limit: MAX_STEP_EXECUTIONS,
            })
        } else {
            Ok(())
        }
    }

    /// Whether the run has already used up [`MAX_STEP_EXECUTIONS`].
    fn step_executions_exhausted(&self) -> bool {
        self.step_executions
            .load(std::sync::atomic::Ordering::Relaxed)
            >= MAX_STEP_EXECUTIONS
    }

    /// Check if we can safely recurse into a step
    fn can_recurse(&self, step_id: &str) -> Result<(), String> {
        if self.recursion_depth >= MAX_RECURSION_DEPTH {
            return Err(format!(
                "Maximum recursion depth ({}) exceeded",
                MAX_RECURSION_DEPTH
            ));
        }
        if self.executing_steps.contains(&step_id.to_string()) {
            return Err(format!(
                "Circular reference detected: step '{}' is already executing",
                step_id
            ));
        }
        Ok(())
    }

    /// Enter a nested step execution
    fn enter_step(&mut self, step_id: &str) {
        self.recursion_depth += 1;
        self.executing_steps.push(step_id.to_string());
    }

    /// Exit a nested step execution
    fn exit_step(&mut self) {
        self.recursion_depth = self.recursion_depth.saturating_sub(1);
        self.executing_steps.pop();
    }

    /// Typed dependency error for clear handling
    ///
    /// When `current_iteration` is Some(idx), performs iteration-aware lookup:
    /// first tries `dep@idx` (same-iteration result), then falls back to plain `dep`
    /// (aggregate or pre-loop result).
    fn check_dependencies(
        &self,
        step: &WorkflowStep,
        all_step_ids: &std::collections::HashSet<String>,
        current_iteration: Option<usize>,
    ) -> Result<(), DependencyError> {
        for dep in &step.depends_on {
            // First verify the dependency is a known step ID
            if !all_step_ids.contains(dep) {
                return Err(DependencyError::Unknown(dep.clone()));
            }

            // Iteration-aware lookup: try dep@idx first if in loop context
            let result = if let Some(idx) = current_iteration {
                let iter_key = format!("{}@{}", dep, idx);
                self.step_results
                    .get(&iter_key)
                    .or_else(|| self.step_results.get(dep))
            } else {
                self.step_results.get(dep)
            };

            match result {
                Some(result) if result.status == StepStatus::Completed => continue,
                Some(result) => {
                    return Err(DependencyError::NotSatisfied {
                        dep: dep.clone(),
                        status: result.status,
                    });
                }
                None => {
                    return Err(DependencyError::NotExecuted(dep.clone()));
                }
            }
        }
        Ok(())
    }
}

/// Typed dependency error for proper handling
#[derive(Debug, Clone)]
pub enum DependencyError {
    /// Dependency ID doesn't exist in workflow definition (always fatal)
    Unknown(String),
    /// Dependency exists but hasn't been executed yet
    NotExecuted(String),
    /// Dependency executed but not completed (failed/skipped)
    NotSatisfied { dep: String, status: StepStatus },
}

impl std::fmt::Display for DependencyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DependencyError::Unknown(dep) => write!(f, "Unknown dependency: '{}'", dep),
            DependencyError::NotExecuted(dep) => write!(f, "Dependency '{}' not yet executed", dep),
            DependencyError::NotSatisfied { dep, status } => {
                write!(
                    f,
                    "Dependency '{}' not satisfied (status: {:?})",
                    dep, status
                )
            }
        }
    }
}

impl DependencyError {
    /// Returns true if this is a definition error (should always fail)
    pub fn is_definition_error(&self) -> bool {
        matches!(self, DependencyError::Unknown(_))
    }
}

impl WorkflowContext {
    /// Set variable
    pub fn set_var(&mut self, name: impl Into<String>, value: impl Into<VarValue>) {
        self.variables.insert(name.into(), value.into());
    }

    /// Get variable
    pub fn get_var(&self, name: &str) -> Option<&VarValue> {
        self.variables.get(name)
    }

    /// Substitute variables in a string
    pub fn substitute(&self, template: &str) -> String {
        Self::substitute_impl(template, &self.variables, false)
    }

    /// Substitute variables with shell-safe quoting to prevent injection.
    ///
    /// Each value is wrapped in single quotes with internal single quotes
    /// escaped as `'\''`, which is the standard POSIX shell quoting approach.
    /// Use this for any string that will be passed to `sh -c`.
    pub fn substitute_shell_safe(&self, template: &str) -> String {
        Self::substitute_impl(template, &self.variables, true)
    }

    fn substitute_impl(
        template: &str,
        variables: &HashMap<String, VarValue>,
        shell_quote: bool,
    ) -> String {
        // Single left-to-right scan. Two properties the old replace-in-a-loop
        // approach lacked:
        //
        // 1. Deterministic longest match: with both `$item` and `$item_count`
        //    defined, `$item_count` always resolves to `item_count` — the old
        //    code iterated a HashMap and, whenever `item` came first, rewrote
        //    `$item_count` to `<item value>_count` (which then shell-executed).
        // 2. Substituted values are never re-scanned, so a value containing
        //    `$other` is inserted literally instead of being re-substituted
        //    in a later, order-dependent pass.
        let mut result = String::with_capacity(template.len());
        let mut rest = template;

        while let Some(pos) = rest.find('$') {
            result.push_str(&rest[..pos]);
            rest = &rest[pos..];

            // ${name} form
            if let Some(after_open) = rest.strip_prefix("${") {
                match after_open.find('}') {
                    Some(end) => {
                        let name = &after_open[..end];
                        match variables.get(name).and_then(|v| v.as_string()) {
                            Some(value) => {
                                result.push_str(&Self::maybe_shell_quote(&value, shell_quote))
                            }
                            // Unknown variable: keep the placeholder verbatim
                            None => result.push_str(&rest[..end + 3]),
                        }
                        rest = &after_open[end + 1..];
                    }
                    // Unclosed placeholder: emit verbatim and keep scanning
                    None => {
                        result.push_str("${");
                        rest = after_open;
                    }
                }
                continue;
            }

            // $name form: the longest variable name matching at this position
            // wins, regardless of HashMap iteration order.
            let best = variables
                .iter()
                .filter(|(name, value)| {
                    !name.is_empty()
                        && value.as_string().is_some()
                        && rest[1..].starts_with(name.as_str())
                })
                .max_by_key(|(name, _)| name.len());

            match best {
                Some((name, value)) => {
                    let value = value.as_string().unwrap_or_default();
                    result.push_str(&Self::maybe_shell_quote(&value, shell_quote));
                    rest = &rest[1 + name.len()..];
                }
                // Lone '$' (no variable matches here): emit verbatim
                None => {
                    result.push('$');
                    rest = &rest[1..];
                }
            }
        }

        result.push_str(rest);
        result
    }

    /// Apply POSIX shell quoting when the substitution target is a shell command.
    fn maybe_shell_quote(value: &str, shell_quote: bool) -> String {
        if shell_quote {
            Self::shell_quote(value)
        } else {
            value.to_string()
        }
    }

    /// POSIX shell quoting: wrap in single quotes, escape internal single quotes.
    fn shell_quote(s: &str) -> String {
        // Single-quoted strings in POSIX shell treat everything as literal
        // except single quotes themselves. Escape them by ending the quoted
        // section, adding an escaped single quote, and reopening.
        format!("'{}'", s.replace('\'', "'\\''"))
    }

    /// Evaluate a simple condition
    pub fn evaluate_condition(&self, condition: &str) -> bool {
        let condition = self.substitute(condition);

        // Simple evaluations
        if condition == "true" {
            return true;
        }
        if condition == "false" {
            return false;
        }

        // Check for variable existence. A malformed expression (missing the
        // closing paren) fails CLOSED instead of falling through to the
        // truthiness check below, which would report it as true.
        if let Some(inner) = condition.strip_prefix("defined(") {
            return match inner.strip_suffix(')') {
                Some(var_name) => self.variables.contains_key(var_name),
                None => false,
            };
        }

        // Check for step success (same fail-closed policy on malformed input)
        if let Some(inner) = condition.strip_prefix("success(") {
            return match inner.strip_suffix(')') {
                Some(step_id) => self
                    .step_results
                    .get(step_id)
                    .map(|r| r.status == StepStatus::Completed)
                    .unwrap_or(false),
                None => false,
            };
        }

        // Check for step failure (same fail-closed policy on malformed input)
        if let Some(inner) = condition.strip_prefix("failed(") {
            return match inner.strip_suffix(')') {
                Some(step_id) => self
                    .step_results
                    .get(step_id)
                    .map(|r| r.status == StepStatus::Failed)
                    .unwrap_or(false),
                None => false,
            };
        }

        // Simple equality check. Malformed equality (anything other than
        // exactly two operands) fails closed rather than falling through to
        // the truthiness check.
        if condition.contains("==") {
            let parts: Vec<&str> = condition.split("==").collect();
            return parts.len() == 2 && parts[0].trim() == parts[1].trim();
        }

        // Non-empty check
        !condition.is_empty() && condition != "0"
    }

    /// Log a message.
    ///
    /// When the number of log entries exceeds MAX_WORKFLOW_LOG_ENTRIES, the
    /// oldest entries are removed to stay within the limit.
    pub fn log(&mut self, level: LogLevel, message: impl Into<String>, step_id: Option<String>) {
        self.logs.push_back(LogEntry {
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            level,
            message: message.into(),
            step_id,
        });
        while self.logs.len() > MAX_WORKFLOW_LOG_ENTRIES {
            self.logs.pop_front();
        }
    }

    /// Get elapsed time in milliseconds
    pub fn elapsed_ms(&self) -> u64 {
        self.started_at
            .map(|s| s.elapsed().as_millis() as u64)
            .unwrap_or(0)
    }

    /// The stop reason if the running workflow has used up its budget.
    pub fn budget_exceeded(&self) -> Option<WorkflowStopReason> {
        self.budget.exceeded(self.elapsed_ms(), &self.telemetry)
    }

    /// Budget check before a control-flow body step (loop iteration, `until`
    /// pass, condition branch): once the run is over budget (or already
    /// stopped on one), record the typed stop reason and return it as the
    /// error, so the body stops before starting a step it cannot afford —
    /// the same stop the top-level loop makes between steps.
    fn stop_if_over_budget(&mut self, before: impl FnOnce() -> String) -> Result<()> {
        if let Some(reason) = self.stop_reason.clone().or_else(|| self.budget_exceeded()) {
            self.log(
                LogLevel::Error,
                format!("Workflow stopped before {}: {}", before(), reason),
                None,
            );
            self.stop_reason = Some(reason.clone());
            return Err(reason.into());
        }
        Ok(())
    }

    fn record_llm_call(&mut self, output: &LlmCallOutput, latency_ms: u64) {
        self.telemetry.llm_calls += 1;
        self.telemetry.llm_latency_ms += latency_ms;

        let usage = output.usage.unwrap_or_default();
        self.telemetry.prompt_tokens += usage.prompt_tokens;
        self.telemetry.completion_tokens += usage.completion_tokens;
        self.telemetry.total_tokens += usage.total_tokens;
        self.telemetry.add_reported_cost(output.cost_usd);

        if output.usage.is_none() {
            self.telemetry.unmetered_llm_calls += 1;
        }

        if usage.total_tokens > 0 {
            add_tokens_processed(usage.total_tokens);
        }

        record_workflow_llm_call(
            self.workflow_name.as_deref().unwrap_or("unknown"),
            output.model.as_deref().unwrap_or("unknown"),
            latency_ms,
            usage.prompt_tokens,
            usage.completion_tokens,
            usage.total_tokens,
            output.cost_usd,
        );
    }

    fn finalize_telemetry(&mut self) {
        self.telemetry.completed_steps = self
            .step_results
            .values()
            .filter(|result| result.status == StepStatus::Completed)
            .count() as u64;
        self.telemetry.failed_steps = self
            .step_results
            .values()
            .filter(|result| result.status == StepStatus::Failed)
            .count() as u64;
        self.telemetry.skipped_steps = self
            .step_results
            .values()
            .filter(|result| result.status == StepStatus::Skipped)
            .count() as u64;
    }
}

/// Type alias for tool handler function
/// Async tool handler for workflow Tool steps (review finding: the previous
/// sync handler type forced a `block_in_place`/`block_on` bridge inside the
/// workflow executor, which froze a runtime worker and defeated the
/// workflow's `tokio::select!` timeout).
pub type ToolHandler = Box<
    dyn Fn(&str, &HashMap<String, String>) -> futures::future::BoxFuture<'static, Result<String>>
        + Send
        + Sync,
>;

/// Async LLM handler for workflow Llm steps.
///
/// Must return a future rather than a finished result: the step runs inside
/// a `tokio::select!` against its timeout, and a sync handler (the previous
/// type, bridged with `block_in_place`/`block_on` in the CLI) blocked the very
/// first poll of the step future until the model answered, so the timeout arm
/// could never fire for Llm steps — the same defect already fixed for
/// [`ToolHandler`].
pub type LlmHandler = Box<
    dyn Fn(&str, &[String]) -> futures::future::BoxFuture<'static, Result<LlmCallOutput>>
        + Send
        + Sync,
>;

/// Upper bound on a single retry backoff sleep. `delay_secs * 2^(n-1)`
/// overflowed (panic in debug, wrap to 0 in release) and a large configured
/// delay slept practically forever outside any timeout.
pub const MAX_BACKOFF_SECS: u64 = 60;

/// Upper bound on `retry.max_attempts`; larger values are clamped (with a
/// warning) when a workflow is registered.
pub const MAX_RETRY_ATTEMPTS: u32 = 10;

/// Backoff before retry `attempt` (1-based: the first retry is attempt 1).
/// Saturating and clamped to [`MAX_BACKOFF_SECS`] for any input.
pub fn retry_backoff_secs(retry: &RetryConfig, attempt: u32) -> u64 {
    let delay = if retry.exponential {
        let factor = 2u64
            .checked_pow(attempt.saturating_sub(1))
            .unwrap_or(u64::MAX);
        retry.delay_secs.saturating_mul(factor)
    } else {
        retry.delay_secs
    };
    delay.min(MAX_BACKOFF_SECS)
}

/// Clamp every step's `retry.max_attempts` to [`MAX_RETRY_ATTEMPTS`],
/// warning for each step that asked for more.
fn clamp_retry_attempts(workflow: &mut Workflow) {
    for step in &mut workflow.steps {
        if step.retry.max_attempts > MAX_RETRY_ATTEMPTS {
            tracing::warn!(
                workflow = %workflow.name,
                step = %step.id,
                requested = step.retry.max_attempts,
                limit = MAX_RETRY_ATTEMPTS,
                "workflow step retry.max_attempts clamped"
            );
            step.retry.max_attempts = MAX_RETRY_ATTEMPTS;
        }
    }
}

/// Workflow executor
pub struct WorkflowExecutor {
    /// Registered workflows
    workflows: HashMap<String, Workflow>,
    /// Tool execution handler (injected)
    tool_handler: Option<ToolHandler>,
    /// LLM execution handler (injected)
    llm_handler: Option<LlmHandler>,
    /// Dry-run mode (log but don't execute)
    dry_run: bool,
    /// Safety checker for validating shell commands before execution
    safety_checker: crate::safety::SafetyChecker,
    /// Per-workflow resource budgets (by workflow name)
    budgets: HashMap<String, WorkflowBudget>,
}

impl WorkflowExecutor {
    /// Create new executor in live mode
    pub fn new() -> Self {
        Self {
            workflows: HashMap::new(),
            tool_handler: None,
            llm_handler: None,
            dry_run: false,
            safety_checker: crate::safety::SafetyChecker::new(
                &crate::config::SafetyConfig::default(),
            ),
            budgets: HashMap::new(),
        }
    }

    /// Create new executor in dry-run mode
    pub fn new_dry_run() -> Self {
        Self {
            workflows: HashMap::new(),
            tool_handler: None,
            llm_handler: None,
            dry_run: true,
            safety_checker: crate::safety::SafetyChecker::new(
                &crate::config::SafetyConfig::default(),
            ),
            budgets: HashMap::new(),
        }
    }

    /// Create new executor in live mode using the provided safety config.
    pub fn new_with_config(safety_config: &crate::config::SafetyConfig) -> Self {
        Self {
            workflows: HashMap::new(),
            tool_handler: None,
            llm_handler: None,
            dry_run: false,
            safety_checker: crate::safety::SafetyChecker::new(safety_config),
            budgets: HashMap::new(),
        }
    }

    /// Create new executor in dry-run mode using the provided safety config.
    pub fn new_dry_run_with_config(safety_config: &crate::config::SafetyConfig) -> Self {
        Self {
            dry_run: true,
            ..Self::new_with_config(safety_config)
        }
    }

    /// Set tool handler for executing tool steps
    pub fn with_tool_handler(mut self, handler: ToolHandler) -> Self {
        self.tool_handler = Some(handler);
        self
    }

    /// Set a synchronous LLM handler for executing LLM steps.
    ///
    /// The closure runs on the blocking thread pool (`spawn_blocking`) so a
    /// slow handler cannot stall the executor's poll and the step timeout
    /// still fires. On timeout the step fails and the closure's result is
    /// discarded (a blocking thread cannot be cancelled mid-call). Prefer
    /// [`Self::with_async_llm_handler`] for real network clients.
    pub fn with_llm_handler<F, R>(mut self, handler: F) -> Self
    where
        F: Fn(&str, &[String]) -> Result<R> + Send + Sync + 'static,
        R: Into<LlmCallOutput>,
    {
        let handler = std::sync::Arc::new(handler);
        self.llm_handler = Some(Box::new(move |prompt, context| {
            let handler = std::sync::Arc::clone(&handler);
            let prompt = prompt.to_string();
            let context = context.to_vec();
            Box::pin(async move {
                crate::tools::workspace_root::spawn_blocking(move || {
                    handler(&prompt, &context).map(Into::into)
                })
                .await
                .map_err(|e| anyhow!("LLM handler task failed: {}", e))?
            })
        }));
        self
    }

    /// Set an async LLM handler for executing LLM steps. The returned future
    /// is awaited inside the step's timeout `select!`, so dropping it on
    /// timeout cancels the in-flight request.
    pub fn with_async_llm_handler(mut self, handler: LlmHandler) -> Self {
        self.llm_handler = Some(handler);
        self
    }

    /// Register a workflow
    pub fn register(&mut self, mut workflow: Workflow) {
        clamp_retry_attempts(&mut workflow);
        self.workflows.insert(workflow.name.clone(), workflow);
    }

    /// Set (or, with an unbounded budget, clear) the resource budget of the
    /// workflow named `name`.
    pub fn set_budget(&mut self, name: impl Into<String>, budget: WorkflowBudget) {
        let name = name.into();
        if budget.is_unbounded() {
            self.budgets.remove(&name);
        } else {
            self.budgets.insert(name, budget);
        }
    }

    /// The resource budget of the workflow named `name`.
    pub fn budget(&self, name: &str) -> WorkflowBudget {
        self.budgets.get(name).copied().unwrap_or_default()
    }

    /// Load workflow from YAML string (including its top-level
    /// `max_wall_secs` / `max_tokens` budget, if any).
    pub fn load_yaml(&mut self, yaml: &str) -> Result<()> {
        let workflow: Workflow = serde_yaml::from_str(yaml)
            .map_err(|e| anyhow!("Failed to parse workflow YAML: {}", e))?;
        let budget: WorkflowBudget = serde_yaml::from_str(yaml)
            .map_err(|e| anyhow!("Failed to parse workflow budget: {}", e))?;
        self.set_budget(workflow.name.clone(), budget);
        self.register(workflow);
        Ok(())
    }

    /// Load workflow from file
    pub fn load_file(&mut self, path: &Path) -> Result<()> {
        let content = std::fs::read_to_string(path)?;
        self.load_yaml(&content)
    }

    /// Get workflow by name
    pub fn get(&self, name: &str) -> Option<&Workflow> {
        self.workflows.get(name)
    }

    /// List all workflows
    pub fn list(&self) -> Vec<&Workflow> {
        self.workflows.values().collect()
    }

    /// List workflows by category
    pub fn list_by_category(&self, category: &str) -> Vec<&Workflow> {
        self.workflows
            .values()
            .filter(|w| w.category == category)
            .collect()
    }

    /// Execute a workflow
    pub async fn execute(
        &self,
        name: &str,
        inputs: HashMap<String, VarValue>,
        working_dir: PathBuf,
    ) -> Result<WorkflowResult> {
        // Start with empty call stack for top-level execution
        self.execute_with_call_stack(name, inputs, working_dir, Vec::new(), None)
            .await
    }

    /// Execute a workflow as a checkpointed run: after every top-level step
    /// the run's state is written to `run.store`, and when `run` resumes a
    /// checkpoint, its variables are restored and every top-level step that
    /// completed with an unchanged definition is skipped (not re-run, not
    /// re-billed). Explicit `inputs` override restored variables.
    pub async fn execute_run(
        &self,
        name: &str,
        inputs: HashMap<String, VarValue>,
        working_dir: PathBuf,
        run: WorkflowRun,
    ) -> Result<WorkflowResult> {
        self.execute_workflow(name, inputs, working_dir, Vec::new(), None, Some(run))
            .await
    }

    /// Execute a workflow with call stack tracking for cycle detection
    async fn execute_with_call_stack(
        &self,
        name: &str,
        inputs: HashMap<String, VarValue>,
        working_dir: PathBuf,
        call_stack: Vec<String>,
        step_executions: Option<std::sync::Arc<std::sync::atomic::AtomicU64>>,
    ) -> Result<WorkflowResult> {
        self.execute_workflow(name, inputs, working_dir, call_stack, step_executions, None)
            .await
    }

    /// Execute a workflow; `run` enables checkpointing / resume (top-level
    /// runs only — sub-workflows are part of their parent's step).
    async fn execute_workflow(
        &self,
        name: &str,
        inputs: HashMap<String, VarValue>,
        working_dir: PathBuf,
        call_stack: Vec<String>,
        step_executions: Option<std::sync::Arc<std::sync::atomic::AtomicU64>>,
        run: Option<WorkflowRun>,
    ) -> Result<WorkflowResult> {
        // Check for workflow-level cycles
        if call_stack.contains(&name.to_string()) {
            return Err(anyhow!(
                "Workflow cycle detected: {} is already in call stack {:?}",
                name,
                call_stack
            ));
        }

        // Check max workflow nesting depth
        const MAX_WORKFLOW_DEPTH: usize = 10;
        if call_stack.len() >= MAX_WORKFLOW_DEPTH {
            return Err(anyhow!(
                "Maximum workflow nesting depth ({}) exceeded",
                MAX_WORKFLOW_DEPTH
            ));
        }

        let workflow = self
            .workflows
            .get(name)
            .ok_or_else(|| anyhow!("Workflow not found: {}", name))?
            .clone();

        let mut context = WorkflowContext::new(working_dir.clone());
        if let Some(counter) = step_executions {
            context.step_executions = counter;
        }
        context.workflow_name = Some(workflow.name.clone());
        context.started_at = Some(Instant::now());
        context.status = WorkflowStatus::Running;

        // Store call stack in context for sub-workflow calls
        let mut current_stack = call_stack;
        current_stack.push(name.to_string());
        context.workflow_call_stack = current_stack;
        context.budget = self.budget(name);

        // Set input variables
        for (key, value) in inputs {
            context.set_var(&key, value);
        }

        // Resume: restore the checkpoint's variables (explicit inputs win)
        // and its completed step results. Failed/skipped results are not
        // restored — those steps run again and decide the verdict afresh.
        let resumed = run.as_ref().and_then(|run| run.resume_from.as_ref());
        if let Some(checkpoint) = resumed {
            if checkpoint.workflow_name != workflow.name {
                return Err(anyhow!(
                    "run '{}' is a run of workflow '{}', not '{}'",
                    checkpoint.run_id,
                    checkpoint.workflow_name,
                    workflow.name
                ));
            }
            for (key, value) in &checkpoint.variables {
                context
                    .variables
                    .entry(key.clone())
                    .or_insert_with(|| value.clone());
            }
            for (key, result) in checkpoint.completed_results() {
                context.step_results.insert(key.clone(), result.clone());
            }
            context.log(
                LogLevel::Info,
                format!(
                    "Resuming run '{}' ({} completed step(s) in checkpoint)",
                    checkpoint.run_id,
                    checkpoint.completed_steps.len()
                ),
                None,
            );
        }
        let mut completed_steps: std::collections::BTreeMap<String, String> =
            std::collections::BTreeMap::new();

        // Set defaults for missing inputs
        for input in &workflow.inputs {
            if !context.variables.contains_key(&input.name) {
                if let Some(ref default) = input.default {
                    context.set_var(&input.name, default.clone());
                } else if input.required {
                    return Err(anyhow!("Missing required input: {}", input.name));
                }
            }
        }

        // Build set of all step IDs for dependency validation (unified with inline paths)
        let all_step_ids: std::collections::HashSet<String> =
            workflow.steps.iter().map(|s| s.id.clone()).collect();

        // Pre-mark every step referenced by a Condition or Loop as
        // control-flow-managed, regardless of document order. Marking used to
        // happen only when the Condition/Loop executed, so a referenced step
        // declared EARLIER in the document ran once in the top-level pass and
        // then again inline — double-executing shell side effects and
        // double-billing LLM steps.
        for step in &workflow.steps {
            match &step.step_type {
                StepType::Condition {
                    then_steps,
                    else_steps,
                    ..
                } => {
                    for step_id in then_steps {
                        context.control_flow_managed_steps.insert(step_id.clone());
                    }
                    if let Some(else_ids) = else_steps {
                        for step_id in else_ids {
                            context.control_flow_managed_steps.insert(step_id.clone());
                        }
                    }
                }
                StepType::Loop { do_steps, .. } => {
                    for step_id in do_steps {
                        context.control_flow_managed_steps.insert(step_id.clone());
                    }
                }
                StepType::Until { do_steps, .. } => {
                    for step_id in do_steps {
                        context.control_flow_managed_steps.insert(step_id.clone());
                    }
                }
                _ => {}
            }
        }

        // Execute steps
        'step_loop: for (idx, step) in workflow.steps.iter().enumerate() {
            context.current_step = idx;

            // Budget check between steps: stop before starting a step the
            // run can no longer afford; results so far are kept.
            if let Some(reason) = context
                .stop_reason
                .clone()
                .or_else(|| context.budget_exceeded())
            {
                context.status = WorkflowStatus::Failed;
                context.log(
                    LogLevel::Error,
                    format!("Workflow stopped before step '{}': {}", step.id, reason),
                    Some(step.id.clone()),
                );
                context.stop_reason = Some(reason);
                break 'step_loop;
            }

            // Skip steps that were already executed inline by control-flow (condition/loop)
            if context.control_flow_managed_steps.contains(&step.id) {
                context.log(
                    LogLevel::Debug,
                    format!(
                        "Skipping step {} (already executed inline by control-flow)",
                        step.id
                    ),
                    Some(step.id.clone()),
                );
                continue 'step_loop;
            }

            // Resume: a step that completed in the checkpointed run, with the
            // same definition, is not run (or billed) again.
            if let Some(previous) = resumed.and_then(|cp| cp.completed_steps.get(&step.id)) {
                let fingerprint = step_fingerprint(step);
                let restored = context
                    .step_results
                    .get(&step.id)
                    .is_some_and(|r| r.status == StepStatus::Completed);
                if *previous == fingerprint && restored {
                    context.log(
                        LogLevel::Info,
                        format!(
                            "Step '{}' completed in the resumed run; not re-run",
                            step.id
                        ),
                        Some(step.id.clone()),
                    );
                    completed_steps.insert(step.id.clone(), fingerprint);
                    continue 'step_loop;
                }
                context.log(
                    LogLevel::Warn,
                    format!(
                        "Step '{}' changed since the checkpoint; re-running it",
                        step.id
                    ),
                    Some(step.id.clone()),
                );
            }

            // Check dependencies using unified check_dependencies (no iteration context at top level)
            if let Err(dep_err) = context.check_dependencies(step, &all_step_ids, None) {
                // Definition errors (unknown deps) are always fatal
                if dep_err.is_definition_error() {
                    return Err(anyhow!(
                        "Step '{}' has invalid dependency: {}",
                        step.id,
                        dep_err
                    ));
                }

                // For required steps, dependency failures are hard failures
                if step.required {
                    context.status = WorkflowStatus::Failed;
                    context.log(
                        LogLevel::Error,
                        format!(
                            "Required step '{}' cannot run due to unsatisfied dependency: {}",
                            step.id, dep_err
                        ),
                        Some(step.id.clone()),
                    );
                    context.step_results.insert(
                        step.id.clone(),
                        StepResult {
                            step_id: step.id.clone(),
                            status: StepStatus::Failed,
                            output: None,
                            error: Some(dep_err.to_string()),
                            duration_ms: 0,
                            retry_count: 0,
                        },
                    );
                    break 'step_loop;
                }

                // Optional step with runtime dep failure - skip
                context.log(
                    LogLevel::Warn,
                    format!(
                        "Skipping optional step {} due to dependency: {}",
                        step.id, dep_err
                    ),
                    Some(step.id.clone()),
                );
                context.step_results.insert(
                    step.id.clone(),
                    StepResult {
                        step_id: step.id.clone(),
                        status: StepStatus::Skipped,
                        output: None,
                        error: Some(dep_err.to_string()),
                        duration_ms: 0,
                        retry_count: 0,
                    },
                );
                continue 'step_loop;
            }

            // Execute step with retries (pass all workflow steps for nested execution)
            let result = self
                .execute_step_with_retry(step, &mut context, &workflow.steps)
                .await;

            context.step_results.insert(step.id.clone(), result.clone());

            if result.status == StepStatus::Completed {
                if let Some(output) = result.output.clone() {
                    context.set_var(&step.id, output);
                }
                completed_steps.insert(step.id.clone(), step_fingerprint(step));
            } else {
                completed_steps.remove(&step.id);
            }
            if let Some(run) = &run {
                checkpoint::save_progress(run, &workflow.name, &mut context, &completed_steps);
            }

            // Check if we should abort
            if result.status == StepStatus::Failed && step.required {
                context.status = WorkflowStatus::Failed;
                context.log(
                    LogLevel::Error,
                    format!("Workflow failed at step: {}", step.id),
                    Some(step.id.clone()),
                );
                break;
            }
        }

        // Set final status if not already failed
        if context.status == WorkflowStatus::Running {
            // Steps executed inline by control flow (condition/loop branches)
            // record failures in step_results keyed by step id (loops use
            // "id@iteration") without tripping the top-level abort check
            // above — e.g. a required step failing inside an optional loop.
            // Any failed (non-skipped) step that is required (non-optional)
            // must fail the workflow: never report success when a required
            // step failed.
            let required_step_failed = context.step_results.iter().any(|(key, result)| {
                result.status == StepStatus::Failed
                    && workflow.steps.iter().any(|s| {
                        s.required && s.id == key.split('@').next().unwrap_or(key.as_str())
                    })
            });
            context.status = if required_step_failed || context.stop_reason.is_some() {
                WorkflowStatus::Failed
            } else {
                WorkflowStatus::Completed
            };
        }

        // Collect outputs
        let mut outputs = HashMap::new();
        for output in &workflow.outputs {
            if let Some(value) = context.get_var(&output.from) {
                outputs.insert(output.name.clone(), value.clone());
            }
        }

        if context.budget.max_tokens.is_some() && context.telemetry.unmetered_llm_calls > 0 {
            context.log(
                LogLevel::Warn,
                format!(
                    "max_tokens budget covered only metered calls: {} LLM call(s) reported no token usage",
                    context.telemetry.unmetered_llm_calls
                ),
                None,
            );
        }

        let duration_ms = context.elapsed_ms();
        context.finalize_telemetry();
        if let Some(run) = &run {
            checkpoint::save_progress(run, &workflow.name, &mut context, &completed_steps);
        }
        let telemetry = context.telemetry.clone();
        record_workflow_run(
            &workflow.name,
            match context.status {
                WorkflowStatus::Completed => "completed",
                WorkflowStatus::Failed => "failed",
                WorkflowStatus::Paused => "paused",
                WorkflowStatus::Cancelled => "cancelled",
                WorkflowStatus::Running => "running",
                WorkflowStatus::Pending => "pending",
            },
            duration_ms,
            telemetry.llm_calls,
            telemetry.prompt_tokens,
            telemetry.completion_tokens,
            telemetry.total_tokens,
            telemetry.cost_usd,
        );

        Ok(WorkflowResult {
            workflow_name: workflow.name,
            status: context.status,
            outputs,
            step_results: context.step_results,
            logs: context.logs,
            duration_ms,
            telemetry,
            stop_reason: context.stop_reason,
            run_id: run.map(|run| run.run_id),
        })
    }

    /// Execute a single step with retry logic
    async fn execute_step_with_retry(
        &self,
        step: &WorkflowStep,
        context: &mut WorkflowContext,
        workflow_steps: &[WorkflowStep],
    ) -> StepResult {
        let start = Instant::now();
        if let Err(limit) = context.charge_step_execution() {
            context.log(
                LogLevel::Error,
                format!("Step {} not run: {}", step.id, limit),
                Some(step.id.clone()),
            );
            return StepResult {
                step_id: step.id.clone(),
                status: StepStatus::Failed,
                output: None,
                error: Some(limit.to_string()),
                duration_ms: 0,
                retry_count: 0,
            };
        }
        let max_attempts = step.retry.max_attempts.clamp(1, MAX_RETRY_ATTEMPTS);
        let mut last_error = None;
        let mut attempts_made: u32 = 0;

        // Apply timeout if specified
        let timeout_duration = step
            .timeout_secs
            .map(Duration::from_secs)
            .unwrap_or_else(|| default_step_timeout(&step.step_type));

        // Total wall-time budget for the step: one timeout per allowed
        // attempt. Backoff sleeps are charged against it, so retries can
        // never run longer than `timeout × max_attempts` in total.
        let deadline = start.checked_add(timeout_duration.saturating_mul(max_attempts));

        for attempt in 0..max_attempts {
            let remaining = deadline.map(|d| d.saturating_duration_since(Instant::now()));
            if attempt > 0 {
                let delay = Duration::from_secs(retry_backoff_secs(&step.retry, attempt));
                if remaining.is_some_and(|rem| delay >= rem) {
                    let budget_secs = timeout_duration.saturating_mul(max_attempts).as_secs();
                    let reason = format!(
                        "retry budget of {}s (timeout {}s x {} attempts) exhausted before attempt {}",
                        budget_secs,
                        timeout_duration.as_secs(),
                        max_attempts,
                        attempt + 1
                    );
                    context.log(
                        LogLevel::Warn,
                        format!("Step {}: {}", step.id, reason),
                        Some(step.id.clone()),
                    );
                    last_error = Some(match last_error {
                        Some(prev) => format!("{prev} ({reason})"),
                        None => reason,
                    });
                    break;
                }
                tokio::time::sleep(delay).await;

                context.log(
                    LogLevel::Info,
                    format!("Retrying step {} (attempt {})", step.id, attempt + 1),
                    Some(step.id.clone()),
                );
            }
            attempts_made = attempt + 1;

            // Each attempt gets its own timeout, but never more than what is
            // left of the step's total budget.
            let attempt_timeout = deadline
                .map(|d| timeout_duration.min(d.saturating_duration_since(Instant::now())))
                .unwrap_or(timeout_duration);

            // Use tokio::select! to ensure the step future is explicitly
            // dropped (cancelled) when the timeout fires, preventing
            // background work from continuing after timeout.
            // The step_fut borrow of `context` must end before we can
            // use `context` again, so we scope the select block.
            let execution_result = {
                let step_fut =
                    self.execute_step_with_workflow(&step.step_type, context, Some(workflow_steps));
                tokio::pin!(step_fut);

                tokio::select! {
                    result = &mut step_fut => Some(result),
                    _ = tokio::time::sleep(attempt_timeout) => {
                        // Timeout fired: step_fut is dropped at end of this
                        // block, cancelling it. In-flight shell steps run via
                        // `output_grouped`, so their whole process group is
                        // killed, not just the direct shell.
                        None
                    }
                }
                // step_fut is dropped here, releasing the borrow on context
            };

            match execution_result {
                Some(Ok(output)) => {
                    return StepResult {
                        step_id: step.id.clone(),
                        status: StepStatus::Completed,
                        output: Some(output),
                        error: None,
                        duration_ms: start.elapsed().as_millis() as u64,
                        retry_count: attempt,
                    };
                }
                Some(Err(e)) => {
                    last_error = Some(e.to_string());
                    context.log(
                        LogLevel::Warn,
                        format!("Step {} failed: {}", step.id, e),
                        Some(step.id.clone()),
                    );
                    // A budget stop is final: another attempt cannot
                    // un-spend the budget, it would only stop again.
                    if context.stop_reason.is_some() {
                        break;
                    }
                }
                None => {
                    // Timeout elapsed — step future has been cancelled
                    last_error = Some(format!(
                        "Step timed out after {} seconds",
                        attempt_timeout.as_secs()
                    ));
                    context.log(
                        LogLevel::Warn,
                        format!(
                            "Step {} timed out after {}s — task cancelled",
                            step.id,
                            attempt_timeout.as_secs()
                        ),
                        Some(step.id.clone()),
                    );
                }
            }
        }

        StepResult {
            step_id: step.id.clone(),
            status: StepStatus::Failed,
            output: None,
            error: last_error,
            duration_ms: start.elapsed().as_millis() as u64,
            retry_count: attempts_made.saturating_sub(1),
        }
    }

    /// Execute a single step (test helper for isolated step testing)
    #[cfg(test)]
    async fn execute_step_inner(
        &self,
        step_type: &StepType,
        context: &mut WorkflowContext,
    ) -> Result<VarValue> {
        self.execute_step_with_workflow(step_type, context, None)
            .await
    }

    /// Execute a single step with optional workflow context for nested step execution
    async fn execute_step_with_workflow(
        &self,
        step_type: &StepType,
        context: &mut WorkflowContext,
        workflow_steps: Option<&[WorkflowStep]>,
    ) -> Result<VarValue> {
        match step_type {
            StepType::SetVar { name, value } => {
                let resolved = context.substitute(value);
                context.set_var(name, resolved.clone());
                Ok(VarValue::String(resolved))
            }

            StepType::Log { message, level } => {
                let resolved = context.substitute(message);
                context.log(*level, &resolved, None);
                Ok(VarValue::String(resolved))
            }

            StepType::Condition {
                condition,
                then_steps,
                else_steps,
            } => {
                // Mark ALL branch steps as control-flow-managed BEFORE execution
                // This prevents unselected branch steps from running in top-level pass
                for step_id in then_steps {
                    context.control_flow_managed_steps.insert(step_id.clone());
                }
                if let Some(else_ids) = else_steps {
                    for step_id in else_ids {
                        context.control_flow_managed_steps.insert(step_id.clone());
                    }
                }

                let result = context.evaluate_condition(condition);
                let step_ids = if result {
                    then_steps.clone()
                } else {
                    else_steps.clone().unwrap_or_default()
                };

                // Execute the selected branch steps if workflow context is available
                if let Some(steps) = workflow_steps {
                    // Build set of all step IDs for dependency validation
                    let all_step_ids: std::collections::HashSet<String> =
                        steps.iter().map(|s| s.id.clone()).collect();

                    let mut results = Vec::new();
                    for step_id in &step_ids {
                        // Check for recursion safety
                        context
                            .can_recurse(step_id)
                            .map_err(|e| anyhow!("Recursion error in condition: {}", e))?;

                        if let Some(step) = steps.iter().find(|s| &s.id == step_id) {
                            // Check dependencies before execution (with known step validation)
                            // Condition branches are not iteration-aware, pass None
                            if let Err(dep_err) =
                                context.check_dependencies(step, &all_step_ids, None)
                            {
                                // Definition errors (unknown deps) are always fatal
                                if dep_err.is_definition_error() {
                                    return Err(anyhow!(
                                        "Step '{}' has invalid dependency: {}",
                                        step_id,
                                        dep_err
                                    ));
                                }

                                // For required steps, all dependency errors are hard failures
                                if step.required {
                                    return Err(anyhow!(
                                        "Required step '{}' has unsatisfied dependency: {}",
                                        step_id,
                                        dep_err
                                    ));
                                }

                                // Optional step with runtime dep failure - skip
                                context.log(
                                    LogLevel::Warn,
                                    format!(
                                        "Skipping optional step {} in condition branch: {}",
                                        step_id, dep_err
                                    ),
                                    Some(step_id.clone()),
                                );
                                context.step_results.insert(
                                    step_id.clone(),
                                    StepResult {
                                        step_id: step_id.clone(),
                                        status: StepStatus::Skipped,
                                        output: None,
                                        error: Some(dep_err.to_string()),
                                        duration_ms: 0,
                                        retry_count: 0,
                                    },
                                );
                                continue;
                            }

                            context.stop_if_over_budget(|| {
                                format!("condition branch step '{step_id}'")
                            })?;

                            context.log(
                                LogLevel::Info,
                                format!(
                                    "Condition branch executing step: {} (condition={}, depth={})",
                                    step_id, result, context.recursion_depth
                                ),
                                Some(step_id.clone()),
                            );

                            // Track recursion
                            context.enter_step(step_id);

                            // Execute through full step runner to get retry/timeout/step_results
                            let step_result =
                                Box::pin(self.execute_step_with_retry(step, context, steps)).await;

                            context.exit_step();

                            // Write to step_results for dependency resolution
                            context
                                .step_results
                                .insert(step_id.clone(), step_result.clone());

                            if step_result.status == StepStatus::Completed {
                                results.push(step_result.output.unwrap_or(VarValue::Null));
                            } else if step.required {
                                return Err(anyhow!(
                                    "Required step '{}' failed in condition branch: {}",
                                    step_id,
                                    step_result.error.unwrap_or_default()
                                ));
                            }
                        } else {
                            return Err(anyhow!("Condition references unknown step: {}", step_id));
                        }
                    }
                    // Return last result or null if no steps
                    Ok(results.pop().unwrap_or(VarValue::Null))
                } else {
                    // No workflow context - return step IDs for inspection/dry-run
                    Ok(VarValue::List(
                        step_ids.into_iter().map(VarValue::String).collect(),
                    ))
                }
            }

            StepType::Input {
                prompt,
                variable,
                default,
            } => {
                // In non-interactive mode, use default or fail
                if let Some(ref default_val) = default {
                    context.set_var(variable, default_val.clone());
                    Ok(VarValue::String(default_val.clone()))
                } else {
                    Err(anyhow!(
                        "Interactive input required for '{}' but not available: {}",
                        variable,
                        prompt
                    ))
                }
            }

            StepType::Shell {
                command,
                working_dir,
            } => {
                let resolved_cmd = context.substitute_shell_safe(command);
                let dir = working_dir
                    .as_ref()
                    .map(|d| context.substitute(d))
                    .unwrap_or_else(|| context.working_dir.to_string_lossy().to_string());

                // Validate working_dir is within the project scope
                let dir_path = std::path::Path::new(&dir);
                let canonical_scope = context
                    .working_dir
                    .canonicalize()
                    .unwrap_or_else(|_| context.working_dir.clone());
                if let Ok(canonical_dir) = dir_path.canonicalize() {
                    if !canonical_dir.starts_with(&canonical_scope) {
                        anyhow::bail!(
                            "Workflow working_dir '{}' is outside project scope '{}'",
                            dir,
                            context.working_dir.display()
                        );
                    }
                } else if dir_path.is_absolute() && !dir_path.starts_with(&canonical_scope) {
                    // If we can't canonicalize (dir doesn't exist yet), at least check prefix
                    anyhow::bail!(
                        "Workflow working_dir '{}' is outside project scope '{}'",
                        dir,
                        context.working_dir.display()
                    );
                }

                if self.dry_run {
                    context.log(
                        LogLevel::Info,
                        format!("[DRY-RUN] Would execute: {} in {}", resolved_cmd, dir),
                        None,
                    );
                    return Ok(VarValue::String(format!("(dry-run) {}", resolved_cmd)));
                }

                // Safety check before execution
                self.safety_checker.check_shell_command(&resolved_cmd)?;

                // Execute shell command for real
                context.log(
                    LogLevel::Info,
                    format!("Executing: {} in {}", resolved_cmd, dir),
                    None,
                );

                let (shell, flag) = crate::tools::shell_exec::default_shell();
                let mut shell_cmd = Command::new(shell);
                // Workflow shell steps must not inherit the agent process's
                // credentials (SELFWARE_API_KEY, GITHUB_TOKEN, …) — scripts
                // can spawn arbitrary children (review round 6 #5).
                crate::safety::process_env::sanitize_command_env(&mut shell_cmd);
                // …nor run what an untrusted repository configured for git.
                crate::safety::git_exec::apply_shell_git_env(
                    &mut shell_cmd,
                    std::path::Path::new(&dir),
                );
                let shell_future = shell_cmd
                    .arg(flag)
                    .arg(&resolved_cmd)
                    .current_dir(&dir)
                    .kill_on_drop(true)
                    .output_grouped();

                const WORKFLOW_SHELL_TIMEOUT_SECS: u64 = 300;
                let output = tokio::time::timeout(
                    Duration::from_secs(WORKFLOW_SHELL_TIMEOUT_SECS),
                    shell_future,
                )
                .await
                .map_err(|_| {
                    anyhow!(
                        "Command '{}' timed out after {}s",
                        resolved_cmd,
                        WORKFLOW_SHELL_TIMEOUT_SECS
                    )
                })?
                .map_err(|e| anyhow!("Failed to execute command: {}", e))?;

                let stdout = String::from_utf8_lossy(&output.stdout).to_string();
                let stderr = String::from_utf8_lossy(&output.stderr).to_string();

                if !output.status.success() {
                    let code = output.status.code().unwrap_or(-1);
                    context.log(
                        LogLevel::Error,
                        format!("Command failed (exit {}): {}", code, stderr),
                        None,
                    );
                    return Err(anyhow!(
                        "Command '{}' failed with exit code {}: {}",
                        resolved_cmd,
                        code,
                        stderr.trim()
                    ));
                }

                context.log(
                    LogLevel::Info,
                    format!("Command output: {}", stdout.trim()),
                    None,
                );
                Ok(VarValue::String(stdout))
            }

            StepType::Tool { name, args } => {
                let resolved_args: HashMap<String, String> = args
                    .iter()
                    .map(|(k, v)| (k.clone(), context.substitute(v)))
                    .collect();

                if self.dry_run {
                    context.log(
                        LogLevel::Info,
                        format!(
                            "[DRY-RUN] Would call tool: {} with {:?}",
                            name, resolved_args
                        ),
                        None,
                    );
                    return Ok(VarValue::String(format!("(dry-run) tool: {}", name)));
                }

                // Execute tool via handler
                if let Some(ref handler) = self.tool_handler {
                    context.log(
                        LogLevel::Info,
                        format!("Calling tool: {} with {:?}", name, resolved_args),
                        None,
                    );
                    let result = handler(name, &resolved_args).await?;
                    Ok(VarValue::String(result))
                } else {
                    Err(anyhow!(
                        "Tool step '{}' requires a tool_handler - use with_tool_handler() to configure",
                        name
                    ))
                }
            }

            StepType::Llm {
                prompt,
                context: ctx_vars,
            } => {
                let resolved_prompt = context.substitute(prompt);
                let resolved_context: Vec<String> =
                    ctx_vars.iter().map(|c| context.substitute(c)).collect();

                if self.dry_run {
                    context.log(
                        LogLevel::Info,
                        format!(
                            "[DRY-RUN] Would prompt LLM: {} with context: {:?}",
                            resolved_prompt, resolved_context
                        ),
                        None,
                    );
                    return Ok(VarValue::String(format!(
                        "(dry-run) llm: {}",
                        resolved_prompt
                    )));
                }

                // Execute LLM call via handler
                if let Some(ref handler) = self.llm_handler {
                    context.log(
                        LogLevel::Info,
                        format!("Prompting LLM: {}", resolved_prompt),
                        None,
                    );
                    let llm_start = Instant::now();
                    let result = handler(&resolved_prompt, &resolved_context).await?;
                    context.record_llm_call(&result, llm_start.elapsed().as_millis() as u64);
                    Ok(VarValue::String(result.content))
                } else {
                    Err(anyhow!(
                        "LLM step requires an llm_handler - use with_llm_handler() to configure"
                    ))
                }
            }

            StepType::Loop {
                variable,
                items,
                do_steps,
            } => {
                // Mark ALL loop steps as control-flow-managed BEFORE execution
                for step_id in do_steps {
                    context.control_flow_managed_steps.insert(step_id.clone());
                }

                let items_value = context.substitute(items);
                // Simple split by comma for now
                let item_list: Vec<&str> = items_value.split(',').map(|s| s.trim()).collect();
                let iteration_count = item_list.len();
                if iteration_count > MAX_LOOP_ITEMS {
                    return Err(WorkflowLimitError::LoopTooManyItems {
                        variable: variable.clone(),
                        items: iteration_count,
                        limit: MAX_LOOP_ITEMS,
                    }
                    .into());
                }

                context.log(
                    LogLevel::Info,
                    format!(
                        "Loop starting: {} iterations over '{}'",
                        iteration_count, variable
                    ),
                    None,
                );

                let mut last_result = VarValue::Null;
                // Track per-step aggregated results: (completed_count, failed_count, skipped_count)
                let mut step_aggregates: HashMap<String, (u32, u32, u32)> = HashMap::new();

                for (idx, item) in item_list.into_iter().enumerate() {
                    // Abort the whole loop (even when its body steps are
                    // optional) once the run's step budget is spent, so a
                    // nested loop cannot keep spinning through cheap failures.
                    if context.step_executions_exhausted() {
                        return Err(WorkflowLimitError::StepExecutionLimit {
                            limit: MAX_STEP_EXECUTIONS,
                        }
                        .into());
                    }
                    context.set_var(variable, item);
                    context.log(
                        LogLevel::Debug,
                        format!(
                            "Loop iteration {}/{}: {} = {}",
                            idx + 1,
                            iteration_count,
                            variable,
                            item
                        ),
                        None,
                    );

                    // Execute do_steps if workflow context is available
                    if let Some(steps) = workflow_steps {
                        // Build set of all step IDs for dependency validation
                        let all_step_ids: std::collections::HashSet<String> =
                            steps.iter().map(|s| s.id.clone()).collect();

                        for step_id in do_steps {
                            // Check for recursion safety
                            context
                                .can_recurse(step_id)
                                .map_err(|e| anyhow!("Recursion error in loop: {}", e))?;

                            if let Some(step) = steps.iter().find(|s| &s.id == step_id) {
                                // Check dependencies with iteration-aware lookup (dep@idx first, then global dep)
                                if let Err(dep_err) =
                                    context.check_dependencies(step, &all_step_ids, Some(idx))
                                {
                                    // Definition errors (unknown deps) are always fatal
                                    if dep_err.is_definition_error() {
                                        return Err(anyhow!(
                                            "Step '{}' has invalid dependency: {}",
                                            step_id,
                                            dep_err
                                        ));
                                    }

                                    // For required steps, all dependency errors are hard failures
                                    if step.required {
                                        return Err(anyhow!(
                                            "Required step '{}' has unsatisfied dependency in loop iteration {}: {}",
                                            step_id, idx + 1, dep_err
                                        ));
                                    }

                                    // Optional step with runtime dep failure - skip
                                    context.log(
                                        LogLevel::Warn,
                                        format!(
                                            "Skipping optional step {} in loop iteration {}: {}",
                                            step_id,
                                            idx + 1,
                                            dep_err
                                        ),
                                        Some(step_id.clone()),
                                    );
                                    // Store per-iteration result
                                    let iter_key = format!("{}@{}", step_id, idx);
                                    context.step_results.insert(
                                        iter_key,
                                        StepResult {
                                            step_id: step_id.clone(),
                                            status: StepStatus::Skipped,
                                            output: None,
                                            error: Some(dep_err.to_string()),
                                            duration_ms: 0,
                                            retry_count: 0,
                                        },
                                    );
                                    // Track aggregate
                                    let agg =
                                        step_aggregates.entry(step_id.clone()).or_insert((0, 0, 0));
                                    agg.2 += 1; // skipped
                                    continue;
                                }

                                // Budget check inside the loop, before every
                                // body step: a 1000-item loop is one top-level
                                // step, and must not outrun max_tokens /
                                // max_wall_secs by more than one body step.
                                context.stop_if_over_budget(|| {
                                    format!(
                                        "loop iteration {}/{} step '{}'",
                                        idx + 1,
                                        iteration_count,
                                        step_id
                                    )
                                })?;

                                // Track recursion
                                context.enter_step(step_id);

                                // Execute through full step runner to get retry/timeout/step_results
                                let step_result =
                                    Box::pin(self.execute_step_with_retry(step, context, steps))
                                        .await;

                                context.exit_step();

                                // Store per-iteration result only (step_id@iteration)
                                let iter_key = format!("{}@{}", step_id, idx);
                                context.step_results.insert(iter_key, step_result.clone());

                                // Track aggregate
                                let agg =
                                    step_aggregates.entry(step_id.clone()).or_insert((0, 0, 0));
                                match step_result.status {
                                    StepStatus::Completed => agg.0 += 1,
                                    StepStatus::Failed => agg.1 += 1,
                                    StepStatus::Skipped => agg.2 += 1,
                                    _ => {}
                                }

                                if step_result.status == StepStatus::Completed {
                                    last_result = step_result.output.unwrap_or(VarValue::Null);
                                } else if step.required {
                                    return Err(anyhow!(
                                        "Required step '{}' failed in loop iteration {}: {}",
                                        step_id,
                                        idx + 1,
                                        step_result.error.unwrap_or_default()
                                    ));
                                }
                            } else {
                                return Err(anyhow!("Loop references unknown step: {}", step_id));
                            }
                        }
                    }
                }

                // Store aggregated results for each loop step
                for (step_id, (completed, failed, skipped)) in step_aggregates {
                    // Determine overall status: Failed if any failed, Completed if all completed
                    let status = if failed > 0 {
                        StepStatus::Failed
                    } else if skipped > 0 && completed == 0 {
                        StepStatus::Skipped
                    } else {
                        StepStatus::Completed
                    };

                    context.step_results.insert(
                        step_id.clone(),
                        StepResult {
                            step_id: step_id.clone(),
                            status,
                            output: Some(VarValue::String(format!(
                                "loop: {} completed, {} failed, {} skipped",
                                completed, failed, skipped
                            ))),
                            error: if failed > 0 {
                                Some(format!("{} iterations failed", failed))
                            } else {
                                None
                            },
                            duration_ms: 0, // Aggregate doesn't track timing
                            retry_count: 0,
                        },
                    );
                }

                context.log(
                    LogLevel::Info,
                    format!("Loop completed: {} iterations", iteration_count),
                    None,
                );

                Ok(last_result)
            }

            StepType::Until {
                do_steps,
                condition,
                max_iterations,
                on_exhausted,
            } => {
                Box::pin(self.execute_until(
                    do_steps,
                    condition,
                    *max_iterations,
                    *on_exhausted,
                    context,
                    workflow_steps,
                ))
                .await
            }

            StepType::Pause { message } => {
                let resolved = context.substitute(message);
                context.log(LogLevel::Info, format!("Paused: {}", resolved), None);
                // Pause is informational only - execution continues
                // Real interactive pause would require CLI integration
                Ok(VarValue::String("paused".to_string()))
            }

            StepType::SubWorkflow {
                workflow_name,
                inputs,
            } => {
                let resolved_inputs: HashMap<String, VarValue> = inputs
                    .iter()
                    .map(|(k, v)| (k.clone(), VarValue::String(context.substitute(v))))
                    .collect();

                if self.dry_run {
                    context.log(
                        LogLevel::Info,
                        format!(
                            "[DRY-RUN] Would call sub-workflow: {} with {:?}",
                            workflow_name, resolved_inputs
                        ),
                        None,
                    );
                    return Ok(VarValue::String(format!(
                        "(dry-run) sub-workflow: {}",
                        workflow_name
                    )));
                }

                // Execute sub-workflow if registered
                if self.workflows.contains_key(workflow_name) {
                    context.log(
                        LogLevel::Info,
                        format!("Executing sub-workflow: {}", workflow_name),
                        None,
                    );

                    // Use Box::pin to enable async recursion with call stack for cycle detection
                    let sub_result = Box::pin(self.execute_with_call_stack(
                        workflow_name,
                        resolved_inputs,
                        context.working_dir.clone(),
                        context.workflow_call_stack.clone(),
                        Some(std::sync::Arc::clone(&context.step_executions)),
                    ))
                    .await?;

                    // The parent's telemetry (and so its token budget)
                    // covers what the sub-workflow spent.
                    context.telemetry.absorb_llm_usage(&sub_result.telemetry);

                    // Merge sub-workflow outputs into current context
                    for (key, value) in &sub_result.outputs {
                        context.set_var(key, value.clone());
                    }

                    // Log sub-workflow completion
                    context.log(
                        LogLevel::Info,
                        format!(
                            "Sub-workflow '{}' completed with status: {:?}",
                            workflow_name, sub_result.status
                        ),
                        None,
                    );

                    if sub_result.is_success() {
                        Ok(VarValue::String(format!(
                            "sub-workflow {} completed successfully",
                            workflow_name
                        )))
                    } else {
                        Err(anyhow!(
                            "Sub-workflow '{}' failed: {:?}",
                            workflow_name,
                            sub_result.failed_steps()
                        ))
                    }
                } else {
                    Err(anyhow!("Sub-workflow '{}' not found", workflow_name))
                }
            }

            StepType::Guardrail {
                name,
                condition,
                on_violation,
                severity,
                description: _,
            } => {
                use crate::swl::guardrails::{EvaluationResult, GuardrailContext, GuardrailEngine};

                // Build guardrail context from workflow context
                let mut guard_ctx = GuardrailContext::new();

                // Add state variables to guardrail context
                fn var_value_to_json(value: &VarValue) -> serde_json::Value {
                    match value {
                        VarValue::String(s) => serde_json::Value::String(s.clone()),
                        VarValue::Number(n) => serde_json::Value::Number(
                            serde_json::Number::from_f64(*n).unwrap_or_else(|| 0.into()),
                        ),
                        VarValue::Boolean(b) => serde_json::Value::Bool(*b),
                        VarValue::List(items) => {
                            serde_json::Value::Array(items.iter().map(var_value_to_json).collect())
                        }
                        VarValue::Map(m) => serde_json::Value::Object(
                            m.iter()
                                .map(|(k, v)| (k.clone(), var_value_to_json(v)))
                                .collect(),
                        ),
                        VarValue::Null => serde_json::Value::Null,
                    }
                }

                for (key, value) in &context.variables {
                    guard_ctx
                        .state
                        .insert(key.clone(), var_value_to_json(value));
                }

                // Resolve the condition string with variable substitution
                let resolved_condition = context.substitute(condition);

                if self.dry_run {
                    context.log(
                        LogLevel::Info,
                        format!(
                            "[DRY-RUN] Would evaluate guardrail '{}': condition='{}', action='{}'",
                            name, resolved_condition, on_violation
                        ),
                        None,
                    );
                    return Ok(VarValue::String(format!(
                        "(dry-run) guardrail '{}' checked",
                        name
                    )));
                }

                // Create guardrail engine and evaluate condition
                let engine = GuardrailEngine::new();

                // Determine if this is a JSON Logic condition or inline expression
                let result =
                    if resolved_condition.starts_with("[") && resolved_condition.contains("]:") {
                        // Code block condition - parse the language prefix
                        if let Some(end_idx) = resolved_condition.find("]:") {
                            let lang = &resolved_condition[1..end_idx];
                            let code = &resolved_condition[end_idx + 2..];
                            engine.evaluate_code_condition(lang, code, &guard_ctx)
                        } else {
                            EvaluationResult::Error {
                                message: "Invalid code block format in condition".to_string(),
                            }
                        }
                    } else {
                        // Treat as inline expression
                        engine.evaluate_inline_expression(&resolved_condition, &guard_ctx)
                    };

                match &result {
                    EvaluationResult::Pass => {
                        context.log(LogLevel::Info, format!("Guardrail '{}' passed", name), None);
                        Ok(VarValue::String(format!("guardrail '{}' passed", name)))
                    }
                    EvaluationResult::Fail { reason } => {
                        let action = on_violation.as_str();
                        let log_level = match action {
                            "block" => LogLevel::Error,
                            "alert" => LogLevel::Warn,
                            "warn" => LogLevel::Warn,
                            "log" => LogLevel::Info,
                            // Unrecognized action: logged as an error and
                            // treated as `block` below (fail closed)
                            _ => LogLevel::Error,
                        };

                        context.log(
                            log_level,
                            format!(
                                "Guardrail '{}' violated: {} (action={}, severity={})",
                                name, reason, on_violation, severity
                            ),
                            None,
                        );

                        // Only the explicitly non-blocking actions allow the
                        // workflow to continue. "block" — and any
                        // unrecognized action, e.g. a typo like "blok" that
                        // used to be silently downgraded to a log line —
                        // fails the step (fail closed).
                        match action {
                            "warn" | "log" | "alert" => Ok(VarValue::String(format!(
                                "guardrail '{}' violated but action='{}' allows continuation: {}",
                                name, on_violation, reason
                            ))),
                            _ => Err(anyhow!(
                                "Guardrail '{}' blocked execution: {} (action: {}, severity: {})",
                                name,
                                reason,
                                on_violation,
                                severity
                            )),
                        }
                    }
                    EvaluationResult::Error { message } => {
                        context.log(
                            LogLevel::Error,
                            format!("Guardrail '{}' evaluation error: {}", name, message),
                            None,
                        );
                        Err(anyhow!(
                            "Guardrail '{}' evaluation failed: {}",
                            name,
                            message
                        ))
                    }
                }
            }
        }
    }
}

impl WorkflowExecutor {
    /// Run an [`StepType::Until`] loop: body passes until the condition
    /// holds or the (clamped) iteration cap is hit.
    async fn execute_until(
        &self,
        do_steps: &[String],
        condition: &str,
        max_iterations: u32,
        on_exhausted: UntilExhausted,
        context: &mut WorkflowContext,
        workflow_steps: Option<&[WorkflowStep]>,
    ) -> Result<VarValue> {
        for step_id in do_steps {
            context.control_flow_managed_steps.insert(step_id.clone());
        }

        let cap = effective_until_iterations(max_iterations).ok_or_else(|| {
            anyhow!("until loop max_iterations must be at least 1 (got {max_iterations})")
        })?;
        if cap < max_iterations {
            context.log(
                LogLevel::Warn,
                format!(
                    "until loop max_iterations={max_iterations} clamped to the hard ceiling {MAX_UNTIL_ITERATIONS}"
                ),
                None,
            );
        }

        // Without the workflow's steps (isolated step execution) there is
        // no body to run: report the body ids like `condition` does.
        let Some(steps) = workflow_steps else {
            return Ok(VarValue::List(
                do_steps.iter().cloned().map(VarValue::String).collect(),
            ));
        };
        let all_step_ids: std::collections::HashSet<String> =
            steps.iter().map(|s| s.id.clone()).collect();

        for pass in 1..=cap {
            // Bounded by the run-wide step ceiling too: once it is spent,
            // stop the loop even if every body step is optional.
            if context.step_executions_exhausted() {
                return Err(WorkflowLimitError::StepExecutionLimit {
                    limit: MAX_STEP_EXECUTIONS,
                }
                .into());
            }
            context.log(
                LogLevel::Info,
                format!("Until loop pass {pass}/{cap} (until: {condition})"),
                None,
            );

            for step_id in do_steps {
                context
                    .can_recurse(step_id)
                    .map_err(|e| anyhow!("Recursion error in until loop: {}", e))?;
                let step = steps
                    .iter()
                    .find(|s| &s.id == step_id)
                    .ok_or_else(|| anyhow!("Until loop references unknown step: {}", step_id))?;

                // Body steps see the latest pass's results (plain ids are
                // overwritten each pass). An unsatisfied dependency skips
                // the step for THIS pass — the condition decides whether
                // another pass runs; unknown ids are definition errors.
                if let Err(dep_err) = context.check_dependencies(step, &all_step_ids, None) {
                    if dep_err.is_definition_error() {
                        return Err(anyhow!(
                            "Step '{}' has invalid dependency: {}",
                            step_id,
                            dep_err
                        ));
                    }
                    context.log(
                        LogLevel::Warn,
                        format!("Skipping step {step_id} in until pass {pass}: {dep_err}"),
                        Some(step_id.clone()),
                    );
                    context.step_results.insert(
                        step_id.clone(),
                        StepResult {
                            step_id: step_id.clone(),
                            status: StepStatus::Skipped,
                            output: None,
                            error: Some(dep_err.to_string()),
                            duration_ms: 0,
                            retry_count: 0,
                        },
                    );
                    continue;
                }

                // Inside the pass too, before every body step (not only
                // between passes): one pass must not outrun the budget by
                // more than the step that crossed it.
                context.stop_if_over_budget(|| format!("until pass {pass} step '{step_id}'"))?;

                context.enter_step(step_id);
                let step_result =
                    Box::pin(self.execute_step_with_retry(step, context, steps)).await;
                context.exit_step();

                if step_result.status == StepStatus::Completed {
                    if let Some(output) = step_result.output.clone() {
                        context.set_var(step_id, output);
                    }
                } else {
                    context.log(
                        LogLevel::Warn,
                        format!(
                            "Step {step_id} did not complete in until pass {pass}: {}",
                            step_result.error.as_deref().unwrap_or("no error recorded")
                        ),
                        Some(step_id.clone()),
                    );
                }
                context.step_results.insert(step_id.clone(), step_result);
            }

            if context.evaluate_condition(condition) {
                context.log(
                    LogLevel::Info,
                    format!("Until condition '{condition}' met after {pass} pass(es)"),
                    None,
                );
                return Ok(VarValue::String(format!(
                    "until: condition met after {pass} pass(es)"
                )));
            }

            // Budget check between passes: a fix loop must not outrun the
            // workflow's wall-clock or token budget.
            if pass < cap {
                if let Some(reason) = context.budget_exceeded() {
                    context.log(
                        LogLevel::Error,
                        format!("Until loop stopped after pass {pass}: {reason}"),
                        None,
                    );
                    context.stop_reason = Some(reason.clone());
                    return Err(reason.into());
                }
            }
        }

        match on_exhausted {
            UntilExhausted::Fail => Err(UntilExhaustedError {
                max_iterations: cap,
                condition: condition.to_string(),
            }
            .into()),
            UntilExhausted::Continue => {
                context.log(
                    LogLevel::Warn,
                    format!(
                        "Until condition '{condition}' not met after {cap} pass(es); continuing (on_exhausted: continue)"
                    ),
                    None,
                );
                Ok(VarValue::String(format!(
                    "until: condition not met after {cap} pass(es) (on_exhausted: continue)"
                )))
            }
        }
    }
}

impl Default for WorkflowExecutor {
    fn default() -> Self {
        Self::new()
    }
}

/// Workflow execution result
#[derive(Debug, Clone)]
pub struct WorkflowResult {
    /// Workflow name
    pub workflow_name: String,
    /// Final status
    pub status: WorkflowStatus,
    /// Output values
    pub outputs: HashMap<String, VarValue>,
    /// Step results
    pub step_results: HashMap<String, StepResult>,
    /// Log entries
    pub logs: VecDeque<LogEntry>,
    /// Total duration in milliseconds
    pub duration_ms: u64,
    /// Aggregated workflow telemetry
    pub telemetry: WorkflowTelemetry,
    /// Why the run stopped early (budget), when it did. Partial results are
    /// still in `step_results` / `outputs`.
    pub stop_reason: Option<WorkflowStopReason>,
    /// Id of the checkpointed run (`execute_run`), resumable with
    /// `selfware workflow run <file> --resume <run-id>`.
    pub run_id: Option<String>,
}

impl WorkflowResult {
    /// Check if workflow succeeded
    pub fn is_success(&self) -> bool {
        self.status == WorkflowStatus::Completed
    }

    /// Get output value
    pub fn get_output(&self, name: &str) -> Option<&VarValue> {
        self.outputs.get(name)
    }

    /// Get failed steps
    pub fn failed_steps(&self) -> Vec<&StepResult> {
        self.step_results
            .values()
            .filter(|r| r.status == StepStatus::Failed)
            .collect()
    }
}

/// Built-in workflow templates
pub struct WorkflowTemplates;
