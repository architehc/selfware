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
mod templates;
#[cfg(test)]
mod tests;

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
