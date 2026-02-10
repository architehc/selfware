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
//! - Variable substitution
//! - Tool integration (via handler injection)
//! - Progress tracking

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use tokio::process::Command;

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
    /// Paused (can be resumed)
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
    SetVar { name: String, value: String },
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
#[derive(Debug, Clone)]
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

/// Maximum recursion depth for nested step execution
const MAX_RECURSION_DEPTH: usize = 10;

/// Workflow execution context
#[derive(Debug, Clone)]
pub struct WorkflowContext {
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
    pub logs: Vec<LogEntry>,
    /// Current recursion depth for nested steps
    pub recursion_depth: usize,
    /// Step IDs currently being executed (for cycle detection)
    pub executing_steps: Vec<String>,
    /// Step IDs executed inline by control-flow (condition/loop) - skip in top-level pass
    pub control_flow_managed_steps: std::collections::HashSet<String>,
    /// Workflow call stack for cycle detection in sub-workflows
    pub workflow_call_stack: Vec<String>,
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
            variables: HashMap::new(),
            working_dir: working_dir.into(),
            step_results: HashMap::new(),
            current_step: 0,
            status: WorkflowStatus::Pending,
            started_at: None,
            logs: Vec::new(),
            recursion_depth: 0,
            executing_steps: Vec::new(),
            control_flow_managed_steps: std::collections::HashSet::new(),
            workflow_call_stack: Vec::new(),
        }
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
        let mut result = template.to_string();

        for (name, value) in &self.variables {
            let placeholder = format!("${{{}}}", name);
            if let Some(s) = value.as_string() {
                result = result.replace(&placeholder, &s);
            }
        }

        // Also support $name syntax
        for (name, value) in &self.variables {
            let placeholder = format!("${}", name);
            if let Some(s) = value.as_string() {
                result = result.replace(&placeholder, &s);
            }
        }

        result
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

        // Check for variable existence
        if condition.starts_with("defined(") && condition.ends_with(")") {
            let var_name = &condition[8..condition.len() - 1];
            return self.variables.contains_key(var_name);
        }

        // Check for step success
        if condition.starts_with("success(") && condition.ends_with(")") {
            let step_id = &condition[8..condition.len() - 1];
            return self
                .step_results
                .get(step_id)
                .map(|r| r.status == StepStatus::Completed)
                .unwrap_or(false);
        }

        // Check for step failure
        if condition.starts_with("failed(") && condition.ends_with(")") {
            let step_id = &condition[7..condition.len() - 1];
            return self
                .step_results
                .get(step_id)
                .map(|r| r.status == StepStatus::Failed)
                .unwrap_or(false);
        }

        // Simple equality check
        if condition.contains("==") {
            let parts: Vec<&str> = condition.split("==").collect();
            if parts.len() == 2 {
                return parts[0].trim() == parts[1].trim();
            }
        }

        // Non-empty check
        !condition.is_empty() && condition != "0"
    }

    /// Log a message
    pub fn log(&mut self, level: LogLevel, message: impl Into<String>, step_id: Option<String>) {
        self.logs.push(LogEntry {
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            level,
            message: message.into(),
            step_id,
        });
    }

    /// Get elapsed time in milliseconds
    pub fn elapsed_ms(&self) -> u64 {
        self.started_at
            .map(|s| s.elapsed().as_millis() as u64)
            .unwrap_or(0)
    }
}

/// Type alias for tool handler function
pub type ToolHandler = Box<dyn Fn(&str, &HashMap<String, String>) -> Result<String> + Send + Sync>;

/// Type alias for LLM handler function
pub type LlmHandler = Box<dyn Fn(&str, &[String]) -> Result<String> + Send + Sync>;

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
}

impl WorkflowExecutor {
    /// Create new executor in live mode
    pub fn new() -> Self {
        Self {
            workflows: HashMap::new(),
            tool_handler: None,
            llm_handler: None,
            dry_run: false,
        }
    }

    /// Create new executor in dry-run mode
    pub fn new_dry_run() -> Self {
        Self {
            workflows: HashMap::new(),
            tool_handler: None,
            llm_handler: None,
            dry_run: true,
        }
    }

    /// Set tool handler for executing tool steps
    pub fn with_tool_handler(mut self, handler: ToolHandler) -> Self {
        self.tool_handler = Some(handler);
        self
    }

    /// Set LLM handler for executing LLM steps
    pub fn with_llm_handler(mut self, handler: LlmHandler) -> Self {
        self.llm_handler = Some(handler);
        self
    }

    /// Register a workflow
    pub fn register(&mut self, workflow: Workflow) {
        self.workflows.insert(workflow.name.clone(), workflow);
    }

    /// Load workflow from YAML string
    pub fn load_yaml(&mut self, yaml: &str) -> Result<()> {
        let workflow: Workflow = serde_yaml::from_str(yaml)
            .map_err(|e| anyhow!("Failed to parse workflow YAML: {}", e))?;
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
        self.execute_with_call_stack(name, inputs, working_dir, Vec::new())
            .await
    }

    /// Execute a workflow with call stack tracking for cycle detection
    async fn execute_with_call_stack(
        &self,
        name: &str,
        inputs: HashMap<String, VarValue>,
        working_dir: PathBuf,
        call_stack: Vec<String>,
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
        context.started_at = Some(Instant::now());
        context.status = WorkflowStatus::Running;

        // Store call stack in context for sub-workflow calls
        let mut current_stack = call_stack;
        current_stack.push(name.to_string());
        context.workflow_call_stack = current_stack;

        // Set input variables
        for (key, value) in inputs {
            context.set_var(&key, value);
        }

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

        // Execute steps
        'step_loop: for (idx, step) in workflow.steps.iter().enumerate() {
            context.current_step = idx;

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
                    format!("Skipping optional step {} due to dependency: {}", step.id, dep_err),
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
            context.status = WorkflowStatus::Completed;
        }

        // Collect outputs
        let mut outputs = HashMap::new();
        for output in &workflow.outputs {
            if let Some(value) = context.get_var(&output.from) {
                outputs.insert(output.name.clone(), value.clone());
            }
        }

        let duration_ms = context.elapsed_ms();

        Ok(WorkflowResult {
            workflow_name: workflow.name,
            status: context.status,
            outputs,
            step_results: context.step_results,
            logs: context.logs,
            duration_ms,
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
        let max_attempts = step.retry.max_attempts.max(1);
        let mut last_error = None;

        for attempt in 0..max_attempts {
            if attempt > 0 {
                // Calculate delay
                let delay = if step.retry.exponential {
                    step.retry.delay_secs * 2u64.pow(attempt - 1)
                } else {
                    step.retry.delay_secs
                };
                tokio::time::sleep(Duration::from_secs(delay)).await;

                context.log(
                    LogLevel::Info,
                    format!("Retrying step {} (attempt {})", step.id, attempt + 1),
                    Some(step.id.clone()),
                );
            }

            // Apply timeout if specified
            let timeout_duration = step
                .timeout_secs
                .map(Duration::from_secs)
                .unwrap_or(Duration::from_secs(300)); // Default 5 min timeout

            let execution_result = tokio::time::timeout(
                timeout_duration,
                self.execute_step_with_workflow(&step.step_type, context, Some(workflow_steps)),
            )
            .await;

            match execution_result {
                Ok(Ok(output)) => {
                    return StepResult {
                        step_id: step.id.clone(),
                        status: StepStatus::Completed,
                        output: Some(output),
                        error: None,
                        duration_ms: start.elapsed().as_millis() as u64,
                        retry_count: attempt,
                    };
                }
                Ok(Err(e)) => {
                    last_error = Some(e.to_string());
                    context.log(
                        LogLevel::Warn,
                        format!("Step {} failed: {}", step.id, e),
                        Some(step.id.clone()),
                    );
                }
                Err(_) => {
                    // Timeout elapsed
                    last_error = Some(format!(
                        "Step timed out after {} seconds",
                        timeout_duration.as_secs()
                    ));
                    context.log(
                        LogLevel::Warn,
                        format!(
                            "Step {} timed out after {}s",
                            step.id,
                            timeout_duration.as_secs()
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
            retry_count: max_attempts - 1,
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
                let resolved_cmd = context.substitute(command);
                let dir = working_dir
                    .as_ref()
                    .map(|d| context.substitute(d))
                    .unwrap_or_else(|| context.working_dir.to_string_lossy().to_string());

                if self.dry_run {
                    context.log(
                        LogLevel::Info,
                        format!("[DRY-RUN] Would execute: {} in {}", resolved_cmd, dir),
                        None,
                    );
                    return Ok(VarValue::String(format!("(dry-run) {}", resolved_cmd)));
                }

                // Execute shell command for real
                context.log(
                    LogLevel::Info,
                    format!("Executing: {} in {}", resolved_cmd, dir),
                    None,
                );

                let output = Command::new("sh")
                    .arg("-c")
                    .arg(&resolved_cmd)
                    .current_dir(&dir)
                    .output()
                    .await
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
                    let result = handler(name, &resolved_args)?;
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
                    let result = handler(&resolved_prompt, &resolved_context)?;
                    Ok(VarValue::String(result))
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
                    ))
                    .await?;

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
    pub logs: Vec<LogEntry>,
    /// Total duration in milliseconds
    pub duration_ms: u64,
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

impl WorkflowTemplates {
    /// TDD workflow template
    pub fn tdd() -> Workflow {
        Workflow {
            name: "tdd".to_string(),
            description: "Test-Driven Development workflow".to_string(),
            version: "1.0.0".to_string(),
            author: "Selfware".to_string(),
            category: "development".to_string(),
            inputs: vec![
                WorkflowInput {
                    name: "feature".to_string(),
                    description: "Feature to implement".to_string(),
                    required: true,
                    default: None,
                    param_type: "string".to_string(),
                },
                WorkflowInput {
                    name: "test_file".to_string(),
                    description: "Test file path".to_string(),
                    required: false,
                    default: Some(VarValue::String("tests/test_feature.rs".to_string())),
                    param_type: "string".to_string(),
                },
            ],
            outputs: vec![WorkflowOutput {
                name: "test_passed".to_string(),
                description: "Whether tests passed".to_string(),
                from: "tests_passed".to_string(),
            }],
            steps: vec![
                WorkflowStep {
                    id: "write_test".to_string(),
                    name: "Write failing test".to_string(),
                    description: "Write a test that fails".to_string(),
                    step_type: StepType::Llm {
                        prompt: "Write a failing test for: ${feature}".to_string(),
                        context: vec!["${test_file}".to_string()],
                    },
                    required: true,
                    retry: RetryConfig::default(),
                    timeout_secs: Some(60),
                    depends_on: vec![],
                },
                WorkflowStep {
                    id: "run_test_red".to_string(),
                    name: "Verify test fails".to_string(),
                    description: "Run test to confirm it fails".to_string(),
                    step_type: StepType::Shell {
                        command: "cargo test".to_string(),
                        working_dir: None,
                    },
                    required: false, // Expected to fail
                    retry: RetryConfig::default(),
                    timeout_secs: Some(120),
                    depends_on: vec!["write_test".to_string()],
                },
                WorkflowStep {
                    id: "implement".to_string(),
                    name: "Implement feature".to_string(),
                    description: "Write code to make test pass".to_string(),
                    step_type: StepType::Llm {
                        prompt: "Implement the feature to make the test pass: ${feature}"
                            .to_string(),
                        context: vec!["${test_file}".to_string()],
                    },
                    required: true,
                    retry: RetryConfig::default(),
                    timeout_secs: Some(120),
                    depends_on: vec!["run_test_red".to_string()],
                },
                WorkflowStep {
                    id: "run_test_green".to_string(),
                    name: "Verify test passes".to_string(),
                    description: "Run test to confirm it passes".to_string(),
                    step_type: StepType::Shell {
                        command: "cargo test".to_string(),
                        working_dir: None,
                    },
                    required: true,
                    retry: RetryConfig {
                        max_attempts: 3,
                        delay_secs: 5,
                        exponential: false,
                    },
                    timeout_secs: Some(120),
                    depends_on: vec!["implement".to_string()],
                },
                WorkflowStep {
                    id: "refactor".to_string(),
                    name: "Refactor if needed".to_string(),
                    description: "Clean up the implementation".to_string(),
                    step_type: StepType::Llm {
                        prompt: "Review and refactor the implementation if needed".to_string(),
                        context: vec![],
                    },
                    required: false,
                    retry: RetryConfig::default(),
                    timeout_secs: Some(60),
                    depends_on: vec!["run_test_green".to_string()],
                },
            ],
            tags: vec![
                "tdd".to_string(),
                "testing".to_string(),
                "development".to_string(),
            ],
        }
    }

    /// Debug workflow template
    pub fn debug() -> Workflow {
        Workflow {
            name: "debug".to_string(),
            description: "Debugging workflow".to_string(),
            version: "1.0.0".to_string(),
            author: "Selfware".to_string(),
            category: "debugging".to_string(),
            inputs: vec![WorkflowInput {
                name: "issue".to_string(),
                description: "Issue or error to debug".to_string(),
                required: true,
                default: None,
                param_type: "string".to_string(),
            }],
            outputs: vec![],
            steps: vec![
                WorkflowStep {
                    id: "reproduce".to_string(),
                    name: "Reproduce issue".to_string(),
                    description: "Attempt to reproduce the issue".to_string(),
                    step_type: StepType::Llm {
                        prompt: "Analyze and try to reproduce: ${issue}".to_string(),
                        context: vec![],
                    },
                    required: true,
                    retry: RetryConfig::default(),
                    timeout_secs: Some(60),
                    depends_on: vec![],
                },
                WorkflowStep {
                    id: "analyze".to_string(),
                    name: "Analyze root cause".to_string(),
                    description: "Find the root cause".to_string(),
                    step_type: StepType::Llm {
                        prompt: "Find the root cause of the issue".to_string(),
                        context: vec![],
                    },
                    required: true,
                    retry: RetryConfig::default(),
                    timeout_secs: Some(120),
                    depends_on: vec!["reproduce".to_string()],
                },
                WorkflowStep {
                    id: "fix".to_string(),
                    name: "Implement fix".to_string(),
                    description: "Fix the issue".to_string(),
                    step_type: StepType::Llm {
                        prompt: "Implement a fix for the root cause".to_string(),
                        context: vec![],
                    },
                    required: true,
                    retry: RetryConfig::default(),
                    timeout_secs: Some(120),
                    depends_on: vec!["analyze".to_string()],
                },
                WorkflowStep {
                    id: "verify".to_string(),
                    name: "Verify fix".to_string(),
                    description: "Verify the fix works".to_string(),
                    step_type: StepType::Shell {
                        command: "cargo test".to_string(),
                        working_dir: None,
                    },
                    required: true,
                    retry: RetryConfig::default(),
                    timeout_secs: Some(120),
                    depends_on: vec!["fix".to_string()],
                },
            ],
            tags: vec!["debug".to_string(), "bugfix".to_string()],
        }
    }

    /// Code review workflow template
    pub fn review() -> Workflow {
        Workflow {
            name: "review".to_string(),
            description: "Code review workflow".to_string(),
            version: "1.0.0".to_string(),
            author: "Selfware".to_string(),
            category: "review".to_string(),
            inputs: vec![WorkflowInput {
                name: "files".to_string(),
                description: "Files to review".to_string(),
                required: true,
                default: None,
                param_type: "string".to_string(),
            }],
            outputs: vec![],
            steps: vec![
                WorkflowStep {
                    id: "check_style".to_string(),
                    name: "Check code style".to_string(),
                    description: "Run linter and formatter checks".to_string(),
                    step_type: StepType::Shell {
                        command: "cargo clippy".to_string(),
                        working_dir: None,
                    },
                    required: true,
                    retry: RetryConfig::default(),
                    timeout_secs: Some(120),
                    depends_on: vec![],
                },
                WorkflowStep {
                    id: "review_logic".to_string(),
                    name: "Review logic".to_string(),
                    description: "Review code logic and design".to_string(),
                    step_type: StepType::Llm {
                        prompt: "Review the following files for logic issues: ${files}".to_string(),
                        context: vec!["${files}".to_string()],
                    },
                    required: true,
                    retry: RetryConfig::default(),
                    timeout_secs: Some(180),
                    depends_on: vec![],
                },
                WorkflowStep {
                    id: "check_security".to_string(),
                    name: "Security review".to_string(),
                    description: "Check for security issues".to_string(),
                    step_type: StepType::Llm {
                        prompt: "Review for security vulnerabilities: ${files}".to_string(),
                        context: vec!["${files}".to_string()],
                    },
                    required: true,
                    retry: RetryConfig::default(),
                    timeout_secs: Some(120),
                    depends_on: vec![],
                },
                WorkflowStep {
                    id: "summarize".to_string(),
                    name: "Summarize findings".to_string(),
                    description: "Create review summary".to_string(),
                    step_type: StepType::Llm {
                        prompt: "Summarize all review findings".to_string(),
                        context: vec![],
                    },
                    required: true,
                    retry: RetryConfig::default(),
                    timeout_secs: Some(60),
                    depends_on: vec![
                        "check_style".to_string(),
                        "review_logic".to_string(),
                        "check_security".to_string(),
                    ],
                },
            ],
            tags: vec!["review".to_string(), "code-quality".to_string()],
        }
    }

    /// Refactor workflow template
    pub fn refactor() -> Workflow {
        Workflow {
            name: "refactor".to_string(),
            description: "Refactoring workflow".to_string(),
            version: "1.0.0".to_string(),
            author: "Selfware".to_string(),
            category: "development".to_string(),
            inputs: vec![
                WorkflowInput {
                    name: "target".to_string(),
                    description: "Code to refactor".to_string(),
                    required: true,
                    default: None,
                    param_type: "string".to_string(),
                },
                WorkflowInput {
                    name: "goal".to_string(),
                    description: "Refactoring goal".to_string(),
                    required: true,
                    default: None,
                    param_type: "string".to_string(),
                },
            ],
            outputs: vec![],
            steps: vec![
                WorkflowStep {
                    id: "run_tests_before".to_string(),
                    name: "Run tests before".to_string(),
                    description: "Ensure tests pass before refactoring".to_string(),
                    step_type: StepType::Shell {
                        command: "cargo test".to_string(),
                        working_dir: None,
                    },
                    required: true,
                    retry: RetryConfig::default(),
                    timeout_secs: Some(120),
                    depends_on: vec![],
                },
                WorkflowStep {
                    id: "analyze".to_string(),
                    name: "Analyze code".to_string(),
                    description: "Analyze code structure".to_string(),
                    step_type: StepType::Llm {
                        prompt: "Analyze ${target} for refactoring to achieve: ${goal}".to_string(),
                        context: vec!["${target}".to_string()],
                    },
                    required: true,
                    retry: RetryConfig::default(),
                    timeout_secs: Some(120),
                    depends_on: vec!["run_tests_before".to_string()],
                },
                WorkflowStep {
                    id: "refactor".to_string(),
                    name: "Apply refactoring".to_string(),
                    description: "Apply the refactoring changes".to_string(),
                    step_type: StepType::Llm {
                        prompt: "Apply the planned refactoring changes".to_string(),
                        context: vec!["${target}".to_string()],
                    },
                    required: true,
                    retry: RetryConfig::default(),
                    timeout_secs: Some(180),
                    depends_on: vec!["analyze".to_string()],
                },
                WorkflowStep {
                    id: "run_tests_after".to_string(),
                    name: "Run tests after".to_string(),
                    description: "Ensure tests still pass".to_string(),
                    step_type: StepType::Shell {
                        command: "cargo test".to_string(),
                        working_dir: None,
                    },
                    required: true,
                    retry: RetryConfig {
                        max_attempts: 2,
                        delay_secs: 5,
                        exponential: false,
                    },
                    timeout_secs: Some(120),
                    depends_on: vec!["refactor".to_string()],
                },
            ],
            tags: vec!["refactor".to_string(), "development".to_string()],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_workflow_status_default() {
        assert_eq!(WorkflowStatus::default(), WorkflowStatus::Pending);
    }

    #[test]
    fn test_step_status_default() {
        assert_eq!(StepStatus::default(), StepStatus::Pending);
    }

    #[test]
    fn test_var_value_conversions() {
        let s: VarValue = "hello".into();
        assert_eq!(s.as_string(), Some("hello".to_string()));

        let b: VarValue = true.into();
        assert_eq!(b.as_bool(), Some(true));

        let n: VarValue = 42.into();
        assert_eq!(n.as_string(), Some("42".to_string()));
    }

    #[test]
    fn test_var_value_as_bool() {
        assert_eq!(VarValue::Boolean(true).as_bool(), Some(true));
        assert_eq!(VarValue::Boolean(false).as_bool(), Some(false));
        assert_eq!(VarValue::String("hello".into()).as_bool(), Some(true));
        assert_eq!(VarValue::String("".into()).as_bool(), Some(false));
        assert_eq!(VarValue::Number(1.0).as_bool(), Some(true));
        assert_eq!(VarValue::Number(0.0).as_bool(), Some(false));
        assert_eq!(VarValue::Null.as_bool(), Some(false));
    }

    #[test]
    fn test_workflow_context_creation() {
        let ctx = WorkflowContext::new("/tmp");
        assert_eq!(ctx.working_dir, PathBuf::from("/tmp"));
        assert!(ctx.variables.is_empty());
        assert_eq!(ctx.status, WorkflowStatus::Pending);
    }

    #[test]
    fn test_workflow_context_variables() {
        let mut ctx = WorkflowContext::new("/tmp");
        ctx.set_var("name", "test");
        ctx.set_var("count", 42);

        assert_eq!(
            ctx.get_var("name").and_then(|v| v.as_string()),
            Some("test".to_string())
        );
    }

    #[test]
    fn test_workflow_context_substitute() {
        let mut ctx = WorkflowContext::new("/tmp");
        ctx.set_var("name", "world");
        ctx.set_var("count", 5);

        let result = ctx.substitute("Hello ${name}, count is ${count}");
        assert_eq!(result, "Hello world, count is 5");
    }

    #[test]
    fn test_workflow_context_substitute_dollar_syntax() {
        let mut ctx = WorkflowContext::new("/tmp");
        ctx.set_var("var", "value");

        let result = ctx.substitute("Test $var here");
        assert_eq!(result, "Test value here");
    }

    #[test]
    fn test_workflow_context_evaluate_condition() {
        let mut ctx = WorkflowContext::new("/tmp");
        ctx.set_var("flag", true);

        assert!(ctx.evaluate_condition("true"));
        assert!(!ctx.evaluate_condition("false"));
        assert!(ctx.evaluate_condition("defined(flag)"));
        assert!(!ctx.evaluate_condition("defined(unknown)"));
    }

    #[test]
    fn test_workflow_context_evaluate_equality() {
        let mut ctx = WorkflowContext::new("/tmp");
        ctx.set_var("x", "hello");

        assert!(ctx.evaluate_condition("hello == hello"));
        assert!(!ctx.evaluate_condition("hello == world"));
    }

    #[test]
    fn test_workflow_context_evaluate_step_success() {
        let mut ctx = WorkflowContext::new("/tmp");
        ctx.step_results.insert(
            "step1".to_string(),
            StepResult {
                step_id: "step1".to_string(),
                status: StepStatus::Completed,
                output: None,
                error: None,
                duration_ms: 100,
                retry_count: 0,
            },
        );

        assert!(ctx.evaluate_condition("success(step1)"));
        assert!(!ctx.evaluate_condition("failed(step1)"));
    }

    #[test]
    fn test_workflow_context_log() {
        let mut ctx = WorkflowContext::new("/tmp");
        ctx.log(LogLevel::Info, "Test message", Some("step1".to_string()));

        assert_eq!(ctx.logs.len(), 1);
        assert_eq!(ctx.logs[0].message, "Test message");
    }

    #[test]
    fn test_workflow_executor_creation() {
        let executor = WorkflowExecutor::new();
        assert!(executor.list().is_empty());
    }

    #[test]
    fn test_workflow_executor_register() {
        let mut executor = WorkflowExecutor::new();
        executor.register(WorkflowTemplates::tdd());

        assert!(executor.get("tdd").is_some());
        assert_eq!(executor.list().len(), 1);
    }

    #[test]
    fn test_workflow_executor_list_by_category() {
        let mut executor = WorkflowExecutor::new();
        executor.register(WorkflowTemplates::tdd());
        executor.register(WorkflowTemplates::debug());
        executor.register(WorkflowTemplates::review());

        let dev_workflows = executor.list_by_category("development");
        assert_eq!(dev_workflows.len(), 1);

        let debug_workflows = executor.list_by_category("debugging");
        assert_eq!(debug_workflows.len(), 1);
    }

    #[test]
    fn test_workflow_templates_tdd() {
        let workflow = WorkflowTemplates::tdd();
        assert_eq!(workflow.name, "tdd");
        assert!(!workflow.steps.is_empty());
        assert!(workflow.tags.contains(&"tdd".to_string()));
    }

    #[test]
    fn test_workflow_templates_debug() {
        let workflow = WorkflowTemplates::debug();
        assert_eq!(workflow.name, "debug");
        assert_eq!(workflow.category, "debugging");
    }

    #[test]
    fn test_workflow_templates_review() {
        let workflow = WorkflowTemplates::review();
        assert_eq!(workflow.name, "review");
        assert!(workflow.inputs.iter().any(|i| i.name == "files"));
    }

    #[test]
    fn test_workflow_templates_refactor() {
        let workflow = WorkflowTemplates::refactor();
        assert_eq!(workflow.name, "refactor");
        assert!(workflow.steps.iter().any(|s| s.id == "run_tests_before"));
        assert!(workflow.steps.iter().any(|s| s.id == "run_tests_after"));
    }

    #[tokio::test]
    async fn test_workflow_execution_missing_input() {
        let mut executor = WorkflowExecutor::new();
        executor.register(WorkflowTemplates::tdd());

        let result = executor
            .execute("tdd", HashMap::new(), PathBuf::from("/tmp"))
            .await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_workflow_execution_with_inputs() {
        let mut executor = WorkflowExecutor::new();
        executor.register(WorkflowTemplates::tdd());

        let mut inputs = HashMap::new();
        inputs.insert(
            "feature".to_string(),
            VarValue::String("test feature".into()),
        );

        let result = executor
            .execute("tdd", inputs, PathBuf::from("/tmp"))
            .await
            .unwrap();

        // Workflow should run (may not complete successfully due to simulated execution)
        assert!(!result.step_results.is_empty());
    }

    #[test]
    fn test_workflow_result_helpers() {
        let result = WorkflowResult {
            workflow_name: "test".to_string(),
            status: WorkflowStatus::Completed,
            outputs: HashMap::from([("out".to_string(), VarValue::String("value".into()))]),
            step_results: HashMap::new(),
            logs: Vec::new(),
            duration_ms: 1000,
        };

        assert!(result.is_success());
        assert!(result.get_output("out").is_some());
        assert!(result.failed_steps().is_empty());
    }

    #[test]
    fn test_step_result() {
        let result = StepResult {
            step_id: "test".to_string(),
            status: StepStatus::Completed,
            output: Some(VarValue::String("output".into())),
            error: None,
            duration_ms: 100,
            retry_count: 0,
        };

        assert_eq!(result.status, StepStatus::Completed);
        assert!(result.error.is_none());
    }

    #[test]
    fn test_retry_config_default() {
        let config = RetryConfig::default();
        assert_eq!(config.max_attempts, 0);
        assert_eq!(config.delay_secs, 0);
        assert!(!config.exponential);
    }

    #[test]
    fn test_workflow_step_required_default() {
        // This tests the default_true function
        let step = WorkflowStep {
            id: "test".to_string(),
            name: "Test".to_string(),
            description: "".to_string(),
            step_type: StepType::Log {
                message: "test".to_string(),
                level: LogLevel::Info,
            },
            required: default_true(),
            retry: RetryConfig::default(),
            timeout_secs: None,
            depends_on: vec![],
        };

        assert!(step.required);
    }

    #[test]
    fn test_log_level_default() {
        assert!(matches!(LogLevel::default(), LogLevel::Info));
    }

    #[test]
    fn test_workflow_yaml_parsing() {
        let yaml = r#"
name: test_workflow
description: A test workflow
version: "1.0.0"
category: test
inputs:
  - name: input1
    description: First input
    required: true
steps:
  - id: step1
    name: First step
    type: log
    message: "Hello ${input1}"
tags:
  - test
"#;

        let mut executor = WorkflowExecutor::new();
        let result = executor.load_yaml(yaml);

        assert!(result.is_ok());
        assert!(executor.get("test_workflow").is_some());
    }

    #[tokio::test]
    async fn test_set_var_step() {
        let mut ctx = WorkflowContext::new("/tmp");
        let executor = WorkflowExecutor::new_dry_run();

        let step_type = StepType::SetVar {
            name: "result".to_string(),
            value: "hello".to_string(),
        };

        let result = executor.execute_step_inner(&step_type, &mut ctx).await;
        assert!(result.is_ok());
        assert_eq!(
            ctx.get_var("result").and_then(|v| v.as_string()),
            Some("hello".to_string())
        );
    }

    #[tokio::test]
    async fn test_condition_step() {
        let mut ctx = WorkflowContext::new("/tmp");
        let executor = WorkflowExecutor::new_dry_run();

        let step_type = StepType::Condition {
            condition: "true".to_string(),
            then_steps: vec!["step1".to_string(), "step2".to_string()],
            else_steps: Some(vec!["step3".to_string()]),
        };

        let result = executor
            .execute_step_inner(&step_type, &mut ctx)
            .await
            .unwrap();
        if let VarValue::List(steps) = result {
            assert_eq!(steps.len(), 2);
        } else {
            panic!("Expected list");
        }
    }

    #[tokio::test]
    async fn test_shell_step() {
        let mut ctx = WorkflowContext::new("/tmp");
        let executor = WorkflowExecutor::new_dry_run();

        let step_type = StepType::Shell {
            command: "echo hello".to_string(),
            working_dir: Some("/tmp".to_string()),
        };

        let result = executor.execute_step_inner(&step_type, &mut ctx).await;
        assert!(result.is_ok());
        assert!(!ctx.logs.is_empty());
    }

    #[tokio::test]
    async fn test_input_step_with_default() {
        let mut ctx = WorkflowContext::new("/tmp");
        let executor = WorkflowExecutor::new_dry_run();

        let step_type = StepType::Input {
            prompt: "Enter name".to_string(),
            variable: "name".to_string(),
            default: Some("default_name".to_string()),
        };

        let result = executor
            .execute_step_inner(&step_type, &mut ctx)
            .await
            .unwrap();
        assert_eq!(result.as_string(), Some("default_name".to_string()));
    }

    #[tokio::test]
    async fn test_input_step_without_default() {
        let mut ctx = WorkflowContext::new("/tmp");
        let executor = WorkflowExecutor::new_dry_run();

        let step_type = StepType::Input {
            prompt: "Enter name".to_string(),
            variable: "name".to_string(),
            default: None,
        };

        let result = executor.execute_step_inner(&step_type, &mut ctx).await;
        assert!(result.is_err()); // No default and not interactive
    }

    // Additional comprehensive tests

    #[test]
    fn test_workflow_status_all_variants() {
        let statuses = [
            WorkflowStatus::Pending,
            WorkflowStatus::Running,
            WorkflowStatus::Completed,
            WorkflowStatus::Failed,
            WorkflowStatus::Paused,
            WorkflowStatus::Cancelled,
        ];

        for status in statuses {
            let _ = format!("{:?}", status);
        }
    }

    #[test]
    fn test_step_status_all_variants() {
        let statuses = [
            StepStatus::Pending,
            StepStatus::Running,
            StepStatus::Completed,
            StepStatus::Failed,
            StepStatus::Skipped,
        ];

        for status in statuses {
            let _ = format!("{:?}", status);
        }
    }

    #[test]
    fn test_var_value_list() {
        let list = VarValue::List(vec![
            VarValue::String("a".into()),
            VarValue::Number(1.0),
            VarValue::Boolean(true),
        ]);

        if let VarValue::List(items) = list {
            assert_eq!(items.len(), 3);
        }
    }

    #[test]
    fn test_var_value_map() {
        let mut map = HashMap::new();
        map.insert("key".into(), VarValue::String("value".into()));

        let var = VarValue::Map(map);
        if let VarValue::Map(m) = var {
            assert!(m.contains_key("key"));
        }
    }

    #[test]
    fn test_var_value_null() {
        let null = VarValue::Null;
        assert_eq!(null.as_bool(), Some(false));
        assert_eq!(null.as_string(), None);
    }

    #[test]
    fn test_var_value_from_string_owned() {
        let var: VarValue = String::from("test").into();
        assert_eq!(var.as_string(), Some("test".to_string()));
    }

    #[test]
    fn test_var_value_clone() {
        let original = VarValue::String("test".into());
        let cloned = original.clone();
        assert_eq!(original.as_string(), cloned.as_string());
    }

    #[test]
    fn test_workflow_context_elapsed() {
        let mut ctx = WorkflowContext::new("/tmp");
        ctx.started_at = Some(std::time::Instant::now());

        std::thread::sleep(std::time::Duration::from_millis(10));

        assert!(ctx.elapsed_ms() > 0);
    }

    #[test]
    fn test_workflow_context_elapsed_not_started() {
        let ctx = WorkflowContext::new("/tmp");
        assert_eq!(ctx.elapsed_ms(), 0);
    }

    #[test]
    fn test_workflow_context_multiple_vars() {
        let mut ctx = WorkflowContext::new("/tmp");
        ctx.set_var("a", "1");
        ctx.set_var("b", "2");
        ctx.set_var("c", "3");

        assert_eq!(ctx.variables.len(), 3);
    }

    #[test]
    fn test_workflow_context_overwrite_var() {
        let mut ctx = WorkflowContext::new("/tmp");
        ctx.set_var("x", "old");
        ctx.set_var("x", "new");

        assert_eq!(
            ctx.get_var("x").and_then(|v| v.as_string()),
            Some("new".to_string())
        );
    }

    #[test]
    fn test_substitute_missing_var() {
        let ctx = WorkflowContext::new("/tmp");
        let result = ctx.substitute("Hello ${name}");
        // Should leave the placeholder as-is if variable doesn't exist
        assert!(result.contains("${name}"));
    }

    #[test]
    fn test_condition_non_empty_string() {
        let ctx = WorkflowContext::new("/tmp");
        assert!(ctx.evaluate_condition("non_empty"));
        assert!(!ctx.evaluate_condition("0"));
        assert!(!ctx.evaluate_condition(""));
    }

    #[test]
    fn test_log_entry() {
        let mut ctx = WorkflowContext::new("/tmp");
        ctx.log(LogLevel::Debug, "Debug msg", None);
        ctx.log(LogLevel::Warn, "Warning", Some("step1".into()));
        ctx.log(LogLevel::Error, "Error", Some("step2".into()));

        assert_eq!(ctx.logs.len(), 3);
    }

    #[test]
    fn test_log_level_variants() {
        let levels = [
            LogLevel::Debug,
            LogLevel::Info,
            LogLevel::Warn,
            LogLevel::Error,
        ];

        for level in levels {
            let _ = format!("{:?}", level);
        }
    }

    #[test]
    fn test_workflow_step_clone() {
        let step = WorkflowStep {
            id: "step1".into(),
            name: "Test Step".into(),
            description: "Desc".into(),
            step_type: StepType::Log {
                message: "msg".into(),
                level: LogLevel::Info,
            },
            required: true,
            retry: RetryConfig::default(),
            timeout_secs: Some(60),
            depends_on: vec!["step0".into()],
        };

        let cloned = step.clone();
        assert_eq!(step.id, cloned.id);
    }

    #[test]
    fn test_retry_config_with_values() {
        let config = RetryConfig {
            max_attempts: 3,
            delay_secs: 5,
            exponential: true,
        };

        assert_eq!(config.max_attempts, 3);
        assert!(config.exponential);
    }

    #[test]
    fn test_workflow_input_clone() {
        let input = WorkflowInput {
            name: "param1".into(),
            description: "A parameter".into(),
            required: true,
            default: Some(VarValue::String("default".into())),
            param_type: "string".into(),
        };

        let cloned = input.clone();
        assert_eq!(input.name, cloned.name);
    }

    #[test]
    fn test_workflow_output_clone() {
        let output = WorkflowOutput {
            name: "result".into(),
            description: "The result".into(),
            from: "result_var".into(),
        };

        let cloned = output.clone();
        assert_eq!(output.name, cloned.name);
    }

    #[test]
    fn test_workflow_clone() {
        let workflow = WorkflowTemplates::tdd();
        let cloned = workflow.clone();
        assert_eq!(workflow.name, cloned.name);
        assert_eq!(workflow.steps.len(), cloned.steps.len());
    }

    #[test]
    fn test_step_result_clone() {
        let result = StepResult {
            step_id: "step1".into(),
            status: StepStatus::Completed,
            output: Some(VarValue::String("output".into())),
            error: None,
            duration_ms: 100,
            retry_count: 0,
        };

        let cloned = result.clone();
        assert_eq!(result.step_id, cloned.step_id);
    }

    #[test]
    fn test_workflow_result_is_success() {
        let result = WorkflowResult {
            workflow_name: "test".into(),
            status: WorkflowStatus::Completed,
            outputs: HashMap::new(),
            step_results: HashMap::new(),
            logs: vec![],
            duration_ms: 1000,
        };

        assert!(result.is_success());
    }

    #[test]
    fn test_workflow_result_is_not_success() {
        let result = WorkflowResult {
            workflow_name: "test".into(),
            status: WorkflowStatus::Failed,
            outputs: HashMap::new(),
            step_results: HashMap::new(),
            logs: vec![],
            duration_ms: 1000,
        };

        assert!(!result.is_success());
    }

    #[test]
    fn test_workflow_result_get_output() {
        let mut outputs = HashMap::new();
        outputs.insert("key".into(), VarValue::String("value".into()));

        let result = WorkflowResult {
            workflow_name: "test".into(),
            status: WorkflowStatus::Completed,
            outputs,
            step_results: HashMap::new(),
            logs: vec![],
            duration_ms: 0,
        };

        assert!(result.get_output("key").is_some());
        assert!(result.get_output("missing").is_none());
    }

    #[test]
    fn test_workflow_result_failed_steps() {
        let mut step_results = HashMap::new();
        step_results.insert(
            "step1".into(),
            StepResult {
                step_id: "step1".into(),
                status: StepStatus::Completed,
                output: None,
                error: None,
                duration_ms: 100,
                retry_count: 0,
            },
        );
        step_results.insert(
            "step2".into(),
            StepResult {
                step_id: "step2".into(),
                status: StepStatus::Failed,
                output: None,
                error: Some("Error".into()),
                duration_ms: 50,
                retry_count: 1,
            },
        );

        let result = WorkflowResult {
            workflow_name: "test".into(),
            status: WorkflowStatus::Failed,
            outputs: HashMap::new(),
            step_results,
            logs: vec![],
            duration_ms: 150,
        };

        let failed = result.failed_steps();
        assert_eq!(failed.len(), 1);
        assert_eq!(failed[0].step_id, "step2");
    }

    #[test]
    fn test_executor_default() {
        let executor = WorkflowExecutor::default();
        assert!(executor.list().is_empty());
    }

    #[test]
    fn test_executor_load_invalid_yaml() {
        let mut executor = WorkflowExecutor::new();
        let result = executor.load_yaml("invalid yaml: [[[");
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_tool_step() {
        let mut ctx = WorkflowContext::new("/tmp");
        ctx.set_var("file", "test.rs");
        let executor = WorkflowExecutor::new_dry_run();

        let step_type = StepType::Tool {
            name: "file_read".into(),
            args: HashMap::from([("path".into(), "${file}".into())]),
        };

        let result = executor.execute_step_inner(&step_type, &mut ctx).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_llm_step() {
        let mut ctx = WorkflowContext::new("/tmp");
        let executor = WorkflowExecutor::new_dry_run();

        let step_type = StepType::Llm {
            prompt: "Explain this code".into(),
            context: vec!["file1.rs".into(), "file2.rs".into()],
        };

        let result = executor.execute_step_inner(&step_type, &mut ctx).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_loop_step() {
        let mut ctx = WorkflowContext::new("/tmp");
        let executor = WorkflowExecutor::new_dry_run();

        let step_type = StepType::Loop {
            variable: "item".into(),
            items: "a, b, c".into(),
            do_steps: vec!["process".into()],
        };

        let result = executor.execute_step_inner(&step_type, &mut ctx).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_pause_step() {
        let mut ctx = WorkflowContext::new("/tmp");
        let executor = WorkflowExecutor::new_dry_run();

        let step_type = StepType::Pause {
            message: "Press enter to continue".into(),
        };

        let result = executor.execute_step_inner(&step_type, &mut ctx).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_sub_workflow_step() {
        let mut ctx = WorkflowContext::new("/tmp");
        let executor = WorkflowExecutor::new_dry_run();

        let step_type = StepType::SubWorkflow {
            workflow_name: "sub_wf".into(),
            inputs: HashMap::from([("param".into(), "value".into())]),
        };

        // In dry-run, sub-workflow returns placeholder even if not registered
        let result = executor.execute_step_inner(&step_type, &mut ctx).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_condition_else_branch() {
        let mut ctx = WorkflowContext::new("/tmp");
        let executor = WorkflowExecutor::new_dry_run();

        let step_type = StepType::Condition {
            condition: "false".into(),
            then_steps: vec!["then1".into()],
            else_steps: Some(vec!["else1".into(), "else2".into()]),
        };

        let result = executor
            .execute_step_inner(&step_type, &mut ctx)
            .await
            .unwrap();
        if let VarValue::List(steps) = result {
            assert_eq!(steps.len(), 2);
        }
    }

    #[test]
    fn test_workflow_serialization() {
        let workflow = WorkflowTemplates::tdd();
        let json = serde_json::to_string(&workflow).unwrap();
        assert!(json.contains("tdd"));
    }

    #[test]
    fn test_step_type_serialization() {
        let step_type = StepType::Log {
            message: "test".into(),
            level: LogLevel::Info,
        };
        let json = serde_json::to_string(&step_type).unwrap();
        assert!(json.contains("log"));
    }

    #[test]
    fn test_workflow_context_clone() {
        let mut ctx = WorkflowContext::new("/tmp");
        ctx.set_var("test", "value");

        let cloned = ctx.clone();
        assert_eq!(ctx.working_dir, cloned.working_dir);
        assert_eq!(ctx.variables.len(), cloned.variables.len());
    }

    #[test]
    fn test_step_status_equality() {
        assert_eq!(StepStatus::Pending, StepStatus::Pending);
        assert_ne!(StepStatus::Pending, StepStatus::Running);
    }

    #[test]
    fn test_workflow_status_equality() {
        assert_eq!(WorkflowStatus::Running, WorkflowStatus::Running);
        assert_ne!(WorkflowStatus::Running, WorkflowStatus::Completed);
    }

    #[test]
    fn test_log_entry_clone() {
        let entry = LogEntry {
            timestamp: 12345,
            level: LogLevel::Info,
            message: "Test".into(),
            step_id: Some("step1".into()),
        };

        let cloned = entry.clone();
        assert_eq!(entry.timestamp, cloned.timestamp);
        assert_eq!(entry.message, cloned.message);
    }

    #[test]
    fn test_var_value_default() {
        let var = VarValue::default();
        assert!(matches!(var, VarValue::Null));
    }

    #[test]
    fn test_workflow_version_default() {
        let version = default_version();
        assert_eq!(version, "1.0.0");
    }

    #[test]
    fn test_workflow_string_type_default() {
        let type_str = default_string_type();
        assert_eq!(type_str, "string");
    }

    // =========================================================================
    // Control-flow execution tests
    // =========================================================================

    #[tokio::test]
    async fn test_condition_executes_then_branch() {
        // Create a workflow with a condition step that references nested steps
        let yaml = r#"
name: test_condition
description: Test condition execution
steps:
  - id: cond
    name: Check flag
    type: condition
    if: "true"
    then:
      - log_then
    else:
      - log_else
  - id: log_then
    name: Log then branch
    type: log
    message: "then_executed"
  - id: log_else
    name: Log else branch
    type: log
    message: "else_executed"
"#;

        let mut executor = WorkflowExecutor::new();
        executor.load_yaml(yaml).unwrap();

        let result = executor
            .execute("test_condition", HashMap::new(), PathBuf::from("/tmp"))
            .await
            .unwrap();

        assert!(result.is_success());
        // The condition step should have executed the then branch
        assert!(result.step_results.contains_key("cond"));
        assert_eq!(result.step_results["cond"].status, StepStatus::Completed);
        // Check logs for then_executed message
        let has_then_log = result
            .logs
            .iter()
            .any(|l| l.message.contains("then_executed"));
        assert!(has_then_log, "Then branch should have been executed");
    }

    #[tokio::test]
    async fn test_condition_executes_else_branch() {
        let yaml = r#"
name: test_else
description: Test else branch
steps:
  - id: cond
    name: Check false
    type: condition
    if: "false"
    then:
      - log_then
    else:
      - log_else
  - id: log_then
    name: Log then branch
    type: log
    message: "then_executed"
  - id: log_else
    name: Log else branch
    type: log
    message: "else_executed"
"#;

        let mut executor = WorkflowExecutor::new();
        executor.load_yaml(yaml).unwrap();

        let result = executor
            .execute("test_else", HashMap::new(), PathBuf::from("/tmp"))
            .await
            .unwrap();

        assert!(result.is_success());
        // Check logs for else_executed message
        let has_else_log = result
            .logs
            .iter()
            .any(|l| l.message.contains("else_executed"));
        assert!(has_else_log, "Else branch should have been executed");
    }

    #[tokio::test]
    async fn test_loop_executes_nested_steps() {
        let yaml = r#"
name: test_loop
description: Test loop execution
steps:
  - id: loop
    name: Iterate items
    type: loop
    for: item
    in: "a, b, c"
    do:
      - log_item
  - id: log_item
    name: Log item
    type: log
    message: "Processing: ${item}"
"#;

        let mut executor = WorkflowExecutor::new();
        executor.load_yaml(yaml).unwrap();

        let result = executor
            .execute("test_loop", HashMap::new(), PathBuf::from("/tmp"))
            .await
            .unwrap();

        assert!(result.is_success());
        // Loop should have completed
        assert_eq!(result.step_results["loop"].status, StepStatus::Completed);
        // Check that items were iterated (logged)
        let loop_logs: Vec<_> = result
            .logs
            .iter()
            .filter(|l| l.message.contains("Loop"))
            .collect();
        assert!(!loop_logs.is_empty());
    }

    #[tokio::test]
    async fn test_sub_workflow_execution() {
        let parent_yaml = r#"
name: parent
description: Parent workflow
steps:
  - id: call_child
    name: Call child
    type: sub_workflow
    workflow: child
"#;

        let child_yaml = r#"
name: child
description: Child workflow
steps:
  - id: greet
    name: Greet
    type: log
    message: "Hello from child"
"#;

        let mut executor = WorkflowExecutor::new();
        executor.load_yaml(parent_yaml).unwrap();
        executor.load_yaml(child_yaml).unwrap();

        let result = executor
            .execute("parent", HashMap::new(), PathBuf::from("/tmp"))
            .await
            .unwrap();

        assert!(result.is_success());
        assert_eq!(
            result.step_results["call_child"].status,
            StepStatus::Completed
        );
        // Verify child workflow logs are captured
        let has_child_log = result.logs.iter().any(|l| l.message.contains("child"));
        assert!(has_child_log, "Child workflow should have been executed");
    }

    #[tokio::test]
    async fn test_nested_condition_in_loop() {
        let yaml = r#"
name: nested_control
description: Nested control flow
steps:
  - id: loop
    name: Outer loop
    type: loop
    for: num
    in: "1, 2, 3"
    do:
      - check_num
  - id: check_num
    name: Check number
    type: condition
    if: "true"
    then:
      - log_num
  - id: log_num
    name: Log number
    type: log
    message: "Number: ${num}"
"#;

        let mut executor = WorkflowExecutor::new();
        executor.load_yaml(yaml).unwrap();

        let result = executor
            .execute("nested_control", HashMap::new(), PathBuf::from("/tmp"))
            .await
            .unwrap();

        assert!(result.is_success());
    }

    #[tokio::test]
    async fn test_shell_step_live_execution() {
        // Test that shell steps execute for real (not dry-run)
        let yaml = r#"
name: shell_test
description: Test shell execution
steps:
  - id: echo
    name: Echo test
    type: shell
    command: "echo 'hello world'"
"#;

        let mut executor = WorkflowExecutor::new(); // Live mode
        executor.load_yaml(yaml).unwrap();

        let result = executor
            .execute("shell_test", HashMap::new(), PathBuf::from("/tmp"))
            .await
            .unwrap();

        assert!(result.is_success());
        let step_result = &result.step_results["echo"];
        assert_eq!(step_result.status, StepStatus::Completed);
        // Verify output contains expected text
        if let Some(VarValue::String(output)) = &step_result.output {
            assert!(output.contains("hello world"));
        }
    }

    #[tokio::test]
    async fn test_subworkflow_direct_cycle_detection() {
        // Workflow A calls itself -> direct cycle
        let yaml_a = r#"
name: workflow_a
description: Self-referencing workflow
steps:
  - id: call_self
    name: Call self
    type: sub_workflow
    workflow: workflow_a
    required: true
"#;
        let mut executor = WorkflowExecutor::new();
        executor.load_yaml(yaml_a).unwrap();

        let result = executor
            .execute("workflow_a", HashMap::new(), PathBuf::from("/tmp"))
            .await
            .expect("Workflow execution should return Ok with Failed status");

        // Cycle is detected and workflow fails
        assert_eq!(result.status, WorkflowStatus::Failed);

        // Check that the step error contains cycle detection message
        let step_result = result
            .step_results
            .get("call_self")
            .expect("Step result should exist");
        assert_eq!(step_result.status, StepStatus::Failed);
        let error = step_result.error.as_ref().expect("Error should exist");
        assert!(
            error.contains("cycle") || error.contains("call stack"),
            "Expected cycle detection error, got: {}",
            error
        );
    }

    #[tokio::test]
    async fn test_subworkflow_indirect_cycle_detection() {
        // A -> B -> C -> A (indirect cycle)
        let yaml_a = r#"
name: workflow_a
description: Workflow A
steps:
  - id: call_b
    name: Call B
    type: sub_workflow
    workflow: workflow_b
    required: true
"#;
        let yaml_b = r#"
name: workflow_b
description: Workflow B
steps:
  - id: call_c
    name: Call C
    type: sub_workflow
    workflow: workflow_c
    required: true
"#;
        let yaml_c = r#"
name: workflow_c
description: Workflow C - cycles back to A
steps:
  - id: call_a
    name: Call A
    type: sub_workflow
    workflow: workflow_a
    required: true
"#;
        let mut executor = WorkflowExecutor::new();
        executor.load_yaml(yaml_a).unwrap();
        executor.load_yaml(yaml_b).unwrap();
        executor.load_yaml(yaml_c).unwrap();

        let result = executor
            .execute("workflow_a", HashMap::new(), PathBuf::from("/tmp"))
            .await
            .expect("Workflow execution should return Ok with Failed status");

        // Cycle is detected and workflow fails
        assert_eq!(result.status, WorkflowStatus::Failed);

        // Check logs for cycle detection
        let has_cycle_error = result
            .logs
            .iter()
            .any(|log| log.message.contains("cycle") || log.message.contains("call stack"));
        assert!(
            has_cycle_error,
            "Expected cycle detection in logs, got: {:?}",
            result.logs
        );
    }

    #[tokio::test]
    async fn test_subworkflow_depth_limit() {
        // Create a chain of workflows that exceeds depth limit (10)
        let mut executor = WorkflowExecutor::new();

        for i in 0..12 {
            let next = if i < 11 {
                format!(
                    r#"
  - id: call_next
    name: Call next
    type: sub_workflow
    workflow: workflow_{}
    required: true"#,
                    i + 1
                )
            } else {
                String::new()
            };

            let yaml = format!(
                r#"
name: workflow_{}
description: Workflow {}
steps:{}
  - id: step_{}
    name: Step {}
    type: log
    message: "At depth {}"
"#,
                i, i, next, i, i, i
            );
            executor.load_yaml(&yaml).unwrap();
        }

        let result = executor
            .execute("workflow_0", HashMap::new(), PathBuf::from("/tmp"))
            .await
            .expect("Workflow execution should return Ok with Failed status");

        // Depth limit exceeded and workflow fails
        assert_eq!(result.status, WorkflowStatus::Failed);

        // Check logs for depth limit error
        let has_depth_error = result
            .logs
            .iter()
            .any(|log| log.message.contains("depth") || log.message.contains("exceeded"));
        assert!(
            has_depth_error,
            "Expected depth limit error in logs, got: {:?}",
            result.logs
        );
    }

    #[test]
    fn test_loop_iteration_result_format() {
        // Verify iteration results use step_id@iteration format
        let mut ctx = WorkflowContext::new("/tmp");

        // Simulate what loop execution does
        let step_id = "my_step";
        for i in 0..3 {
            let iter_key = format!("{}@{}", step_id, i);
            ctx.step_results.insert(
                iter_key.clone(),
                StepResult {
                    step_id: step_id.to_string(),
                    status: StepStatus::Completed,
                    output: Some(VarValue::Number(i as f64)),
                    error: None,
                    duration_ms: 100,
                    retry_count: 0,
                },
            );
        }

        // Verify per-iteration results are accessible
        assert!(ctx.step_results.contains_key("my_step@0"));
        assert!(ctx.step_results.contains_key("my_step@1"));
        assert!(ctx.step_results.contains_key("my_step@2"));
    }

    #[test]
    fn test_iteration_aware_dependency_lookup() {
        // Test that check_dependencies with current_iteration looks up dep@idx first
        let mut ctx = WorkflowContext::new("/tmp");

        // Set up step IDs (for validation)
        let all_step_ids: std::collections::HashSet<String> =
            ["step_a", "step_b"].iter().map(|s| s.to_string()).collect();

        // Simulate step_a completed in iteration 2
        ctx.step_results.insert(
            "step_a@2".to_string(),
            StepResult {
                step_id: "step_a".to_string(),
                status: StepStatus::Completed,
                output: None,
                error: None,
                duration_ms: 0,
                retry_count: 0,
            },
        );

        // step_b depends on step_a
        let step_b = WorkflowStep {
            id: "step_b".to_string(),
            name: "Step B".to_string(),
            description: String::new(),
            step_type: StepType::Log {
                message: "test".to_string(),
                level: LogLevel::Info,
            },
            depends_on: vec!["step_a".to_string()],
            required: true,
            timeout_secs: None,
            retry: RetryConfig::default(),
        };

        // Without iteration context (None), step_a is NOT found (only step_a@2 exists)
        let result_no_iter = ctx.check_dependencies(&step_b, &all_step_ids, None);
        assert!(
            result_no_iter.is_err(),
            "Without iteration context, plain 'step_a' should not be found"
        );

        // With iteration context (Some(2)), step_a@2 IS found
        let result_with_iter = ctx.check_dependencies(&step_b, &all_step_ids, Some(2));
        assert!(
            result_with_iter.is_ok(),
            "With iteration 2, step_a@2 should be found: {:?}",
            result_with_iter
        );

        // With wrong iteration context (Some(0)), step_a@0 NOT found, falls back to step_a NOT found
        let result_wrong_iter = ctx.check_dependencies(&step_b, &all_step_ids, Some(0));
        assert!(
            result_wrong_iter.is_err(),
            "With iteration 0, step_a@0 should not be found"
        );

        // Now add a global step_a result (aggregate from previous loop)
        ctx.step_results.insert(
            "step_a".to_string(),
            StepResult {
                step_id: "step_a".to_string(),
                status: StepStatus::Completed,
                output: None,
                error: None,
                duration_ms: 0,
                retry_count: 0,
            },
        );

        // With wrong iteration (Some(0)), should fall back to global step_a
        let result_fallback = ctx.check_dependencies(&step_b, &all_step_ids, Some(0));
        assert!(
            result_fallback.is_ok(),
            "With iteration 0, should fall back to global step_a: {:?}",
            result_fallback
        );
    }

    #[tokio::test]
    async fn test_intra_loop_dependency_execution() {
        // Test that step B in a loop can depend on step A in the SAME iteration
        let yaml = r#"
name: intra_loop_dep_test
version: "1.0"
description: Test intra-loop dependencies

steps:
  - id: loop_test
    name: Loop with deps
    type: loop
    for: item
    in: "1, 2, 3"
    do:
      - step_a
      - step_b

  - id: step_a
    name: Step A
    type: log
    message: "A processing ${item}"

  - id: step_b
    name: Step B
    type: log
    message: "B depends on A for ${item}"
    depends_on:
      - step_a
"#;

        let mut executor = WorkflowExecutor::new();
        executor.load_yaml(yaml).expect("Should parse YAML");

        let result = executor
            .execute("intra_loop_dep_test", HashMap::new(), PathBuf::from("/tmp"))
            .await
            .expect("Workflow should execute");

        // Workflow should complete successfully
        assert_eq!(
            result.status,
            WorkflowStatus::Completed,
            "Workflow should complete. Logs: {:?}",
            result.logs
        );

        // Verify both steps completed for all 3 iterations
        assert!(
            result.step_results.contains_key("step_a@0"),
            "step_a iteration 0 should exist"
        );
        assert!(
            result.step_results.contains_key("step_a@1"),
            "step_a iteration 1 should exist"
        );
        assert!(
            result.step_results.contains_key("step_a@2"),
            "step_a iteration 2 should exist"
        );
        assert!(
            result.step_results.contains_key("step_b@0"),
            "step_b iteration 0 should exist"
        );
        assert!(
            result.step_results.contains_key("step_b@1"),
            "step_b iteration 1 should exist"
        );
        assert!(
            result.step_results.contains_key("step_b@2"),
            "step_b iteration 2 should exist"
        );

        // All iterations of step_b should have completed (not skipped due to missing dep)
        for i in 0..3 {
            let key = format!("step_b@{}", i);
            let step_result = result
                .step_results
                .get(&key)
                .unwrap_or_else(|| panic!("{} should exist", key));
            assert_eq!(
                step_result.status,
                StepStatus::Completed,
                "step_b@{} should be Completed, not {:?}",
                i,
                step_result.status
            );
        }
    }
}
