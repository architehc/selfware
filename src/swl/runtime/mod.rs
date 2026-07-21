//! SWL Workflow Runtime
//!
//! Executes SWL workflows with agent delegation, tool calling, and state management.

pub mod guarded;
pub use guarded::{GuardedRuntimeBuilder, GuardedSwlRuntime};

use crate::api::tool_calling::extract_tool_calls;
use crate::api::{ApiClient, Message, ThinkingMode};
use crate::errors::Result;
use crate::observability::telemetry::{
    add_tokens_processed, increment_api_requests, record_failure, record_state_transition,
    record_success,
};
use crate::orchestration::workflows::VarValue;
use crate::swl::parser::ast::{
    AgentDefinition, ReduceStage, SwlDocument, WorkflowDefinition, WorkflowType,
};
use crate::swl::state::{StateBackendType, StateManager};
use crate::swl::types::schema::StateSchema;
use crate::tools::ToolRegistry;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{debug, error, info, warn, Span};

/// Workflow execution context with persistent state
#[derive(Debug)]
pub struct ExecutionContext {
    /// Current state values (now using JSON values for richer types)
    pub state: HashMap<String, serde_json::Value>,
    /// Execution trace for telemetry
    pub trace: Vec<ExecutionEvent>,
    /// Start time
    pub start_time: std::time::Instant,
    /// State manager for persistence
    state_manager: Option<StateManager>,
}

impl ExecutionContext {
    pub fn new() -> Self {
        Self {
            state: HashMap::new(),
            trace: Vec::new(),
            start_time: std::time::Instant::now(),
            state_manager: None,
        }
    }

    /// Create a new execution context with state persistence
    pub async fn with_persistence(
        workflow_name: &str,
        backend_type: StateBackendType,
    ) -> Result<Self> {
        let mut state_manager =
            StateManager::from_backend_type(backend_type, workflow_name).await?;

        // Load existing state if available
        if let Err(e) = state_manager.load().await {
            warn!("Failed to load state for '{}': {}", workflow_name, e);
        }

        let state = state_manager.get_all().clone();

        Ok(Self {
            state,
            trace: Vec::new(),
            start_time: std::time::Instant::now(),
            state_manager: Some(state_manager),
        })
    }

    /// Create a new execution context with file-based persistence (default location)
    pub async fn with_file_persistence(workflow_name: &str) -> Result<Self> {
        let base_dir = StateBackendType::default_state_dir();
        Self::with_persistence(workflow_name, StateBackendType::File { base_dir }).await
    }

    /// Set the state schema for validation
    pub fn with_schema(mut self, schema: StateSchema) -> Self {
        if let Some(manager) = self.state_manager.take() {
            self.state_manager = Some(manager.with_schema(schema));
        }
        self
    }

    /// Get a state value as string (backward compatible)
    pub fn get(&self, key: &str) -> Option<String> {
        self.state.get(key).map(|v| {
            if let Some(s) = v.as_str() {
                s.to_string()
            } else {
                // Convert other types to string representation
                v.to_string()
            }
        })
    }

    /// Get a state value as JSON
    pub fn get_json(&self, key: &str) -> Option<&serde_json::Value> {
        self.state.get(key)
    }

    /// Set a state value from string (backward compatible)
    pub fn set(&mut self, key: String, value: String) {
        self.state.insert(key, serde_json::Value::String(value));
        self.mark_dirty();
    }

    /// Set a state value as JSON
    pub fn set_json(&mut self, key: String, value: serde_json::Value) {
        self.state.insert(key, value);
        self.mark_dirty();
    }

    /// Mark state as dirty (needs saving)
    fn mark_dirty(&mut self) {
        if let Some(ref mut manager) = self.state_manager {
            // Update the manager's cache
            if let Err(e) = manager.set_all(self.state.clone()) {
                warn!("Failed to update state manager: {}", e);
            }
        }
    }

    /// Persist state to the backend
    pub async fn persist(&mut self) -> Result<()> {
        if let Some(ref mut manager) = self.state_manager {
            // Sync the manager's cache with our state
            manager.set_all(self.state.clone())?;
            manager.save().await?;
            info!("State persisted successfully");
        }
        Ok(())
    }

    /// Load state from the backend
    pub async fn load(&mut self) -> Result<()> {
        if let Some(ref mut manager) = self.state_manager {
            manager.load().await?;
            self.state = manager.get_all().clone();
            info!("State loaded successfully with {} fields", self.state.len());
        }
        Ok(())
    }

    /// Check if state has a key
    pub fn has(&self, key: &str) -> bool {
        self.state.contains_key(key)
    }

    /// Delete a state key
    pub fn delete(&mut self, key: &str) -> bool {
        let removed = self.state.remove(key).is_some();
        if removed {
            self.mark_dirty();
        }
        removed
    }

    /// Get all state keys
    pub fn keys(&self) -> Vec<&String> {
        self.state.keys().collect()
    }

    /// Export state as JSON string
    pub fn export_json(&self) -> Result<String> {
        serde_json::to_string_pretty(&self.state).map_err(|e| {
            crate::errors::SelfwareError::Internal(format!("Failed to serialize state: {}", e))
        })
    }
}

impl Default for ExecutionContext {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for ExecutionContext {
    fn clone(&self) -> Self {
        Self {
            state: self.state.clone(),
            trace: self.trace.clone(),
            start_time: self.start_time,
            state_manager: None, // State manager is not cloned (it's for the primary context)
        }
    }
}

/// Execution event for tracing
#[derive(Debug, Clone)]
pub struct ExecutionEvent {
    pub timestamp: std::time::Instant,
    pub agent: String,
    pub action: String,
    pub duration_ms: u64,
    pub tokens_in: u32,
    pub tokens_out: u32,
}

/// Aggregated telemetry data for an agent
#[derive(Debug, Clone, Default)]
pub struct AgentTelemetry {
    /// Total inference calls made by this agent
    pub total_calls: u64,
    /// Total input tokens consumed
    pub total_tokens_in: u64,
    /// Total output tokens generated
    pub total_tokens_out: u64,
    /// Total latency in milliseconds
    pub total_latency_ms: u64,
    /// Average latency per call (computed)
    pub avg_latency_ms: f64,
}

/// Workflow telemetry summary
#[derive(Debug, Clone, Default)]
pub struct WorkflowTelemetry {
    /// Workflow execution time in milliseconds
    pub workflow_duration_ms: u64,
    /// Per-agent telemetry data
    pub agent_metrics: HashMap<String, AgentTelemetry>,
    /// Total tokens consumed (input + output)
    pub total_tokens: u64,
    /// Total API calls made
    pub total_api_calls: u64,
}

/// SWL Workflow Runtime
pub struct SwlRuntime {
    /// API client for LLM calls
    client: Arc<ApiClient>,
    /// Tool registry for executing tools
    tool_registry: Arc<ToolRegistry>,
    /// Execution context
    context: Arc<Mutex<ExecutionContext>>,
    /// Dry run mode
    dry_run: bool,
    /// Total workflow execution time (updated after each workflow)
    last_workflow_duration_ms: AtomicU64,
    /// Maximum number of tool call iterations per agent execution
    max_tool_iterations: usize,
    /// Workflow name for state persistence
    workflow_name: Option<String>,
    /// Gates every workflow-step tool call the same way the CLI/TUI agent
    /// loop and MCP server do. See `execute_tool_call`.
    safety: crate::safety::SafetyChecker,
}

/// Load the safety config for an SWL runtime. Falls back to defaults (which
/// are already conservative) if no config file is found or it fails to
/// parse, rather than refusing to construct the runtime entirely.
fn load_swl_safety_config() -> crate::config::SafetyConfig {
    match crate::config::Config::load(None) {
        Ok(config) => config.safety,
        Err(e) => {
            tracing::warn!(
                "SWL runtime: failed to load selfware config ({}), using default safety settings",
                e
            );
            crate::config::SafetyConfig::default()
        }
    }
}

impl SwlRuntime {
    /// Create a new runtime with API client
    pub fn new(client: Arc<ApiClient>) -> Self {
        let tool_registry = Arc::new(ToolRegistry::new());
        Self {
            client,
            tool_registry,
            context: Arc::new(Mutex::new(ExecutionContext::new())),
            dry_run: false,
            last_workflow_duration_ms: AtomicU64::new(0),
            max_tool_iterations: 50,
            workflow_name: None,
            safety: crate::safety::SafetyChecker::new(&load_swl_safety_config()),
        }
    }

    /// Create a new runtime with a custom tool registry
    pub fn with_tool_registry(client: Arc<ApiClient>, tool_registry: Arc<ToolRegistry>) -> Self {
        Self {
            client,
            tool_registry,
            context: Arc::new(Mutex::new(ExecutionContext::new())),
            dry_run: false,
            last_workflow_duration_ms: AtomicU64::new(0),
            max_tool_iterations: 50,
            workflow_name: None,
            safety: crate::safety::SafetyChecker::new(&load_swl_safety_config()),
        }
    }

    /// Create a dry-run runtime (no actual LLM calls)
    pub fn new_dry_run() -> Self {
        // For dry-run, we create a minimal config
        let config = crate::config::Config::default();
        let client = Arc::new(ApiClient::new(&config).expect("Failed to create API client"));
        let tool_registry = Arc::new(ToolRegistry::new());
        Self {
            client,
            tool_registry,
            context: Arc::new(Mutex::new(ExecutionContext::new())),
            dry_run: true,
            last_workflow_duration_ms: AtomicU64::new(0),
            max_tool_iterations: 50,
            workflow_name: None,
            safety: crate::safety::SafetyChecker::new(&config.safety),
        }
    }

    /// Create a runtime with file-based state persistence
    pub async fn with_file_persistence(
        client: Arc<ApiClient>,
        workflow_name: &str,
    ) -> Result<Self> {
        let tool_registry = Arc::new(ToolRegistry::new());
        let context = ExecutionContext::with_file_persistence(workflow_name).await?;

        Ok(Self {
            client,
            tool_registry,
            context: Arc::new(Mutex::new(context)),
            dry_run: false,
            last_workflow_duration_ms: AtomicU64::new(0),
            max_tool_iterations: 50,
            workflow_name: Some(workflow_name.to_string()),
            safety: crate::safety::SafetyChecker::new(&load_swl_safety_config()),
        })
    }

    /// Set the maximum number of tool iterations per agent execution
    pub fn with_max_tool_iterations(mut self, max: usize) -> Self {
        self.max_tool_iterations = max;
        self
    }

    /// Execute a workflow from a document with state persistence
    pub async fn execute_workflow(
        &self,
        doc: &SwlDocument,
        workflow_name: &str,
        inputs: HashMap<String, VarValue>,
    ) -> Result<ExecutionResult> {
        info!("Executing workflow: {}", workflow_name);

        // Record workflow start time for telemetry
        let workflow_start = std::time::Instant::now();
        record_state_transition("idle", "executing_workflow");

        // Initialize context with inputs
        {
            let mut ctx = self.context.lock().await;
            for (k, v) in inputs {
                let value = v.as_string().unwrap_or_else(|| format!("{v:?}"));
                ctx.set(k, value);
            }
        }

        // Apply schema defaults if a state schema is defined
        if let Some(ref schema) = doc.state {
            let mut ctx = self.context.lock().await;
            for field in &schema.fields {
                if !ctx.has(&field.name) {
                    if let Some(ref default) = field.default {
                        if let Ok(json_value) = serde_json::to_value(default) {
                            ctx.set_json(field.name.clone(), json_value);
                        }
                    }
                }
            }
        }

        // Find the workflow
        let workflow = doc.workflows.get(workflow_name).ok_or_else(|| {
            crate::errors::SelfwareError::Internal(format!(
                "Workflow '{}' not found in document",
                workflow_name
            ))
        })?;

        // Execute based on workflow type
        let result = match workflow.workflow_type {
            WorkflowType::Sequential => self.execute_sequential(doc, workflow).await,
            WorkflowType::Parallel => self.execute_parallel(doc, workflow).await,
            WorkflowType::MapReduce => self.execute_map_reduce(doc, workflow).await,
            WorkflowType::Conditional => self.execute_conditional(doc, workflow).await,
        };

        // Persist state after workflow execution
        if let Err(e) = self.persist_state().await {
            warn!("Failed to persist workflow state: {}", e);
        }

        // Record workflow completion telemetry
        let duration_ms = workflow_start.elapsed().as_millis() as u64;
        self.last_workflow_duration_ms
            .store(duration_ms, Ordering::Relaxed);

        match &result {
            Ok(_) => {
                record_state_transition("executing_workflow", "completed");
                record_success();
            }
            Err(_) => {
                record_state_transition("executing_workflow", "failed");
                record_failure("workflow execution failed");
            }
        }

        result
    }

    /// Persist the current execution context state
    pub async fn persist_state(&self) -> Result<()> {
        let mut ctx = self.context.lock().await;
        ctx.persist().await
    }

    /// Load state from persistence
    pub async fn load_state(&self) -> Result<()> {
        let mut ctx = self.context.lock().await;
        ctx.load().await
    }

    /// Execute agents in sequence
    async fn execute_sequential(
        &self,
        doc: &SwlDocument,
        _workflow: &WorkflowDefinition,
    ) -> Result<ExecutionResult> {
        debug!("Executing sequential workflow");
        let workflow_start = std::time::Instant::now();

        let mut outputs = HashMap::new();

        // For now, execute all agents in sequence
        for (agent_name, agent) in &doc.agents {
            if self.dry_run {
                println!("   [DRY-RUN] Would execute agent: {}", agent_name);
                continue;
            }

            let output = self.execute_agent(agent_name, agent).await?;
            outputs.insert(agent_name.clone(), output.clone());

            // Store in context
            let mut ctx = self.context.lock().await;
            ctx.set(agent_name.clone(), output);
        }

        let duration_ms = workflow_start.elapsed().as_millis() as u64;

        Ok(ExecutionResult {
            status: ExecutionStatus::Completed,
            outputs,
            duration_ms,
        })
    }

    /// Execute agents in parallel
    async fn execute_parallel(
        &self,
        doc: &SwlDocument,
        _workflow: &WorkflowDefinition,
    ) -> Result<ExecutionResult> {
        debug!("Executing parallel workflow");
        let workflow_start = std::time::Instant::now();

        if self.dry_run {
            for agent_name in doc.agents.keys() {
                println!(
                    "   [DRY-RUN] Would execute agent (parallel): {}",
                    agent_name
                );
            }
            return Ok(ExecutionResult {
                status: ExecutionStatus::Completed,
                outputs: HashMap::new(),
                duration_ms: 0,
            });
        }

        // Spawn all agents concurrently
        let mut handles = Vec::new();

        for (agent_name, agent) in &doc.agents {
            let agent_name = agent_name.clone();
            let agent = agent.clone();
            let client = Arc::clone(&self.client);
            let tool_registry = Arc::clone(&self.tool_registry);
            let context = Arc::clone(&self.context);
            let max_iterations = self.max_tool_iterations;

            let handle = tokio::spawn(async move {
                let runtime = SwlRuntime::with_tool_registry(client, tool_registry)
                    .with_max_tool_iterations(max_iterations);
                let output = runtime.execute_agent(&agent_name, &agent).await?;

                // Store in shared context
                let mut ctx = context.lock().await;
                ctx.set(agent_name.clone(), output.clone());

                Ok::<(String, String), crate::errors::SelfwareError>((agent_name, output))
            });

            handles.push(handle);
        }

        // Collect results
        let mut outputs = HashMap::new();
        let mut had_failure = false;
        for handle in handles {
            match handle.await {
                Ok(Ok((name, output))) => {
                    outputs.insert(name, output);
                }
                Ok(Err(e)) => {
                    error!("Agent failed: {}", e);
                    had_failure = true;
                }
                Err(e) => {
                    error!("Agent task panicked: {}", e);
                    had_failure = true;
                }
            }
        }

        let duration_ms = workflow_start.elapsed().as_millis() as u64;

        Ok(ExecutionResult {
            status: if had_failure {
                ExecutionStatus::Failed
            } else {
                ExecutionStatus::Completed
            },
            outputs,
            duration_ms,
        })
    }

    /// Execute map-reduce workflow
    async fn execute_map_reduce(
        &self,
        doc: &SwlDocument,
        workflow: &WorkflowDefinition,
    ) -> Result<ExecutionResult> {
        debug!("Executing map-reduce workflow");
        let workflow_start = std::time::Instant::now();

        // Map phase: execute in parallel
        let map_result = self.execute_parallel(doc, workflow).await?;

        // Reduce phase: use the configured reduce stage to determine the
        // reducer agent. For ReduceStage::Aggregate the agent name is
        // explicit. For ReduceStage::Code (inline code) there is no named
        // agent, so fall back to the last agent as before.
        if let Some(reduce_agent_name) = workflow.reduce.as_ref().and_then(|reduce| match reduce {
            ReduceStage::Aggregate(agg) => Some(agg.agent.clone()),
            ReduceStage::Code(_) => {
                // No named agent in a code-only reduce stage; fall back to
                // the last agent as the reducer.
                doc.agents.keys().last().map(|s| s.to_string())
            }
        }) {
            if let Some(agent) = doc.agents.get(&reduce_agent_name) {
                if !self.dry_run {
                    let _reduce_output = self.execute_agent(&reduce_agent_name, agent).await?;
                } else {
                    println!(
                        "   [DRY-RUN] Would execute reduce agent: {}",
                        reduce_agent_name
                    );
                }
            }
        }

        let duration_ms = workflow_start.elapsed().as_millis() as u64;

        Ok(ExecutionResult {
            status: ExecutionStatus::Completed,
            outputs: map_result.outputs,
            duration_ms,
        })
    }

    /// Execute conditional workflow
    async fn execute_conditional(
        &self,
        doc: &SwlDocument,
        _workflow: &WorkflowDefinition,
    ) -> Result<ExecutionResult> {
        debug!("Executing conditional workflow");
        let workflow_start = std::time::Instant::now();

        // For now, just execute first agent as condition check
        // Then execute appropriate branch
        if let Some((first_agent_name, first_agent)) = doc.agents.iter().next() {
            let condition_result = if self.dry_run {
                println!(
                    "   [DRY-RUN] Would check condition with agent: {}",
                    first_agent_name
                );
                "true".to_string()
            } else {
                self.execute_agent(first_agent_name, first_agent).await?
            };

            // Evaluate the condition robustly: parse as a boolean rather than
            // naively checking for a substring "true" (which would match
            // "untrue", "true_value", etc.).
            let condition_is_true = {
                let trimmed = condition_result.trim();
                // Exact match on common truthy strings
                trimmed.eq_ignore_ascii_case("true")
                    || trimmed.eq_ignore_ascii_case("yes")
                    || trimmed == "1"
                    || trimmed.eq_ignore_ascii_case("t")
                    || trimmed.eq_ignore_ascii_case("y")
            };
            if condition_is_true {
                // Execute remaining agents
                let mut outputs = HashMap::new();
                for (agent_name, agent) in doc.agents.iter().skip(1) {
                    if self.dry_run {
                        println!("   [DRY-RUN] Would execute agent: {}", agent_name);
                    } else {
                        let output = self.execute_agent(agent_name, agent).await?;
                        outputs.insert(agent_name.clone(), output);
                    }
                }
                let duration_ms = workflow_start.elapsed().as_millis() as u64;
                return Ok(ExecutionResult {
                    status: ExecutionStatus::Completed,
                    outputs,
                    duration_ms,
                });
            }
        }

        let duration_ms = workflow_start.elapsed().as_millis() as u64;

        Ok(ExecutionResult {
            status: ExecutionStatus::Completed,
            outputs: HashMap::new(),
            duration_ms,
        })
    }

    /// Execute a single agent with tool support
    async fn execute_agent(&self, name: &str, agent: &AgentDefinition) -> Result<String> {
        info!("Executing agent: {}", name);

        if self.dry_run {
            return Ok(format!("[DRY-RUN] Agent {} output", name));
        }

        // Get available tools for this agent
        let available_tools = self.get_agent_tools(agent);

        // Build system prompt with tool definitions
        let system_prompt = self.build_system_prompt(agent, &available_tools);

        // Build the initial user message from instruction
        let instruction = agent
            .instruction
            .clone()
            .unwrap_or_else(|| "Complete the task.".to_string());

        // Initialize conversation
        let mut messages = vec![Message::system(system_prompt), Message::user(instruction)];

        // Tool execution loop
        let mut final_response = String::new();
        let mut iteration = 0;

        loop {
            iteration += 1;
            if iteration > self.max_tool_iterations {
                warn!(
                    "Agent {} reached maximum tool iterations ({})",
                    name, self.max_tool_iterations
                );
                final_response.push_str("\n[Note: Reached maximum tool iterations]");
                break;
            }

            // Track inference latency
            let inference_start = std::time::Instant::now();

            // Record API request telemetry
            increment_api_requests();
            let span = tracing::info_span!(
                "agent.inference",
                agent_name = name,
                iteration = iteration,
                duration_ms = tracing::field::Empty,
                tokens_in = tracing::field::Empty,
                tokens_out = tracing::field::Empty,
            );
            let _enter = span.enter();

            // Get tool definitions for the API call
            let tool_definitions = if available_tools.is_empty() {
                None
            } else {
                Some(self.build_tool_definitions(&available_tools))
            };

            // Call LLM
            let response = self
                .client
                .chat(messages.clone(), tool_definitions, ThinkingMode::Disabled)
                .await?;

            let choice = response.choices.into_iter().next().ok_or_else(|| {
                crate::errors::SelfwareError::Internal("Model returned no choices".to_string())
            })?;

            // Calculate inference latency
            let duration_ms = inference_start.elapsed().as_millis() as u64;

            // Record telemetry
            let tokens_in = response.usage.prompt_tokens as u32;
            let tokens_out = response.usage.completion_tokens as u32;
            let total_tokens = (tokens_in + tokens_out) as u64;

            // Update span with telemetry data
            Span::current().record("duration_ms", duration_ms);
            Span::current().record("tokens_in", tokens_in);
            Span::current().record("tokens_out", tokens_out);

            // Record to global telemetry system
            add_tokens_processed(total_tokens);
            record_success();

            let event = ExecutionEvent {
                timestamp: std::time::Instant::now(),
                agent: name.to_string(),
                action: "llm_call".to_string(),
                duration_ms,
                tokens_in,
                tokens_out,
            };

            {
                let mut ctx = self.context.lock().await;
                ctx.trace.push(event);
            }

            // Use the shared extractor: native `tool_calls` if present
            // and non-empty, otherwise fall back to text-format parsing
            // of `content`. This is the same policy used by the main
            // agent and `chat_with_profile`. See `src/api/tool_calling.rs`.
            //
            // SWL doesn't carry a per-runtime native_function_calling
            // flag — `extract_tool_calls` ignores that flag in practice
            // (it always tries native first, then falls back to text),
            // so we pass `true` to make the intent explicit.
            let extracted = extract_tool_calls(&choice.message, true);
            let text = choice.message.content.text_all();

            if !extracted.is_empty() {
                // Add assistant message with the extracted tool calls.
                messages.push(Message {
                    role: "assistant".to_string(),
                    content: crate::api::types::MessageContent::Text(text.clone()),
                    reasoning_content: choice.message.reasoning_content.clone(),
                    tool_calls: Some(extracted.clone()),
                    tool_call_id: None,
                    name: None,
                });

                // Execute each tool call. `extract_tool_calls` already
                // normalized text-format calls into OpenAI-style ToolCall
                // values with synthetic ids, so we have a single code
                // path for both native and text-parsed calls.
                for tool_call in &extracted {
                    let tool_result = self.execute_tool_call(tool_call, &available_tools).await;

                    let result_content = match tool_result {
                        Ok(result) => result.to_string(),
                        Err(e) => format!("Error: {}", e),
                    };

                    // Add tool response message — use the (possibly
                    // synthetic) tool_call_id so backends that demand
                    // matching ids accept the trace.
                    messages.push(Message::tool(result_content, &tool_call.id));
                }

                // Continue the loop for the agent to process tool results
                continue;
            }

            // No tool calls - this is the final response
            final_response = text;
            break;
        }

        Ok(final_response)
    }

    /// Get the list of tools available to an agent based on its definition
    fn get_agent_tools(&self, agent: &AgentDefinition) -> Vec<String> {
        if agent.tools.is_empty() {
            // If no tools specified, return empty list (agent has no tools)
            Vec::new()
        } else {
            agent.tools.clone()
        }
    }

    /// Build system prompt with tool definitions
    fn build_system_prompt(&self, agent: &AgentDefinition, available_tools: &[String]) -> String {
        let base_prompt = agent
            .role
            .clone()
            .unwrap_or_else(|| "You are a helpful assistant.".to_string());

        if available_tools.is_empty() {
            return base_prompt;
        }

        // Build tool descriptions
        let tool_descriptions: Vec<String> = available_tools
            .iter()
            .filter_map(|tool_name| {
                self.tool_registry.get(tool_name).map(|tool| {
                    format!(
                        r#"<tool name="{}">
  <description>{}</description>
  <parameters>{}</parameters>
</tool>"#,
                        tool.name(),
                        tool.description(),
                        tool.schema()
                    )
                })
            })
            .collect();

        if tool_descriptions.is_empty() {
            return base_prompt;
        }

        format!(
            r#"{}

You have access to the following tools:
{}

## Tool Format
To call a tool, use this EXACT XML structure:

<tool>
<name>TOOL_NAME</name>
<arguments>{{"key": "value"}}</arguments>
</tool>

### Example:
<tool>
<name>file_read</name>
<arguments>{{"path": "./src/main.rs"}}</arguments>
</tool>

Wait for tool results before proceeding. When done, respond with plain text only (no tool tags)."#,
            base_prompt,
            tool_descriptions.join("\n")
        )
    }

    /// Build API-compatible tool definitions
    fn build_tool_definitions(
        &self,
        tool_names: &[String],
    ) -> Vec<crate::api::types::ToolDefinition> {
        tool_names
            .iter()
            .filter_map(|name| {
                self.tool_registry
                    .get(name)
                    .map(|tool| crate::api::types::ToolDefinition {
                        def_type: "function".to_string(),
                        function: crate::api::types::FunctionDefinition {
                            name: tool.name().to_string(),
                            description: tool.description().to_string(),
                            parameters: tool.schema(),
                        },
                    })
            })
            .collect()
    }

    /// Execute a native tool call from the API
    async fn execute_tool_call(
        &self,
        tool_call: &crate::api::types::ToolCall,
        available_tools: &[String],
    ) -> anyhow::Result<serde_json::Value> {
        // Check if tool is in available tools list
        if !available_tools.contains(&tool_call.function.name) {
            anyhow::bail!(
                "Tool '{}' is not available to this agent. Available tools: {:?}",
                tool_call.function.name,
                available_tools
            );
        }

        // Same gate the CLI/TUI agent loop and MCP server apply -- without
        // this, an SWL workflow step could invoke shell_exec/file_delete/
        // git_push (or anything else registered) with none of the checks in
        // src/safety/ applied.
        self.safety.check_tool_call(tool_call)?;

        // Parse arguments
        let args: serde_json::Value = serde_json::from_str(&tool_call.function.arguments)?;

        // Execute the tool
        self.tool_registry
            .execute(&tool_call.function.name, args)
            .await
    }

    /// Get execution context
    pub async fn get_context(&self) -> ExecutionContext {
        self.context.lock().await.clone()
    }

    /// Get the execution trace (telemetry events)
    pub async fn get_execution_trace(&self) -> Vec<ExecutionEvent> {
        self.context.lock().await.trace.clone()
    }

    /// Get aggregated telemetry summary
    pub async fn get_telemetry_summary(&self) -> WorkflowTelemetry {
        let ctx = self.context.lock().await;
        let mut summary = WorkflowTelemetry {
            workflow_duration_ms: self.last_workflow_duration_ms.load(Ordering::Relaxed),
            ..Default::default()
        };

        // Aggregate per-agent metrics
        for event in &ctx.trace {
            let agent_metrics = summary
                .agent_metrics
                .entry(event.agent.clone())
                .or_default();

            agent_metrics.total_calls += 1;
            agent_metrics.total_tokens_in += event.tokens_in as u64;
            agent_metrics.total_tokens_out += event.tokens_out as u64;
            agent_metrics.total_latency_ms += event.duration_ms;

            summary.total_tokens += (event.tokens_in + event.tokens_out) as u64;
            summary.total_api_calls += 1;
        }

        // Calculate averages
        for metrics in summary.agent_metrics.values_mut() {
            if metrics.total_calls > 0 {
                metrics.avg_latency_ms =
                    metrics.total_latency_ms as f64 / metrics.total_calls as f64;
            }
        }

        summary
    }

    /// Export telemetry data as JSON string
    pub async fn export_telemetry_json(&self) -> Result<String> {
        let summary = self.get_telemetry_summary().await;

        // Build a serializable representation
        let export = TelemetryExport {
            workflow_duration_ms: summary.workflow_duration_ms,
            total_tokens: summary.total_tokens,
            total_api_calls: summary.total_api_calls,
            agents: summary
                .agent_metrics
                .into_iter()
                .map(|(name, metrics)| AgentTelemetryExport {
                    name,
                    total_calls: metrics.total_calls,
                    total_tokens_in: metrics.total_tokens_in,
                    total_tokens_out: metrics.total_tokens_out,
                    total_latency_ms: metrics.total_latency_ms,
                    avg_latency_ms: metrics.avg_latency_ms,
                })
                .collect(),
        };

        serde_json::to_string_pretty(&export).map_err(|e| {
            crate::errors::SelfwareError::Internal(format!("Failed to serialize telemetry: {}", e))
        })
    }

    /// Clear all telemetry data
    pub async fn clear_telemetry(&self) {
        let mut ctx = self.context.lock().await;
        ctx.trace.clear();
        ctx.start_time = std::time::Instant::now();
        self.last_workflow_duration_ms.store(0, Ordering::Relaxed);
    }

    /// Get the raw execution trace for a specific agent
    pub async fn get_agent_trace(&self, agent_name: &str) -> Vec<ExecutionEvent> {
        let ctx = self.context.lock().await;
        ctx.trace
            .iter()
            .filter(|e| e.agent == agent_name)
            .cloned()
            .collect()
    }
}

/// Serializable telemetry export format
#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct TelemetryExport {
    workflow_duration_ms: u64,
    total_tokens: u64,
    total_api_calls: u64,
    agents: Vec<AgentTelemetryExport>,
}

/// Serializable agent telemetry
#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct AgentTelemetryExport {
    name: String,
    total_calls: u64,
    total_tokens_in: u64,
    total_tokens_out: u64,
    total_latency_ms: u64,
    avg_latency_ms: f64,
}

/// Execution result
#[derive(Debug, Clone)]
pub struct ExecutionResult {
    pub status: ExecutionStatus,
    pub outputs: HashMap<String, String>,
    pub duration_ms: u64,
}

impl Clone for SwlRuntime {
    /// Faithful clone: shares the API client, tool registry, and execution
    /// context (so trace events and state written by the clone are visible to
    /// the original), and preserves `dry_run` / `max_tool_iterations` /
    /// `workflow_name`. The safety checker is rebuilt from the same config.
    ///
    /// `GuardedSwlRuntime` relies on this when spawning parallel agent tasks —
    /// cloning used to substitute a fresh dry-run runtime, so guarded parallel
    /// and map-reduce workflows returned `[DRY-RUN]` placeholder strings while
    /// reporting `Completed`.
    fn clone(&self) -> Self {
        Self {
            client: Arc::clone(&self.client),
            tool_registry: Arc::clone(&self.tool_registry),
            context: Arc::clone(&self.context),
            dry_run: self.dry_run,
            last_workflow_duration_ms: AtomicU64::new(
                self.last_workflow_duration_ms.load(Ordering::Relaxed),
            ),
            max_tool_iterations: self.max_tool_iterations,
            workflow_name: self.workflow_name.clone(),
            safety: crate::safety::SafetyChecker::new(&self.safety.config),
        }
    }
}

/// Execution status
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionStatus {
    Completed,
    Failed,
    Cancelled,
}

#[cfg(test)]
#[path = "../../../tests/unit/swl/runtime/mod_test.rs"]
mod tests;
