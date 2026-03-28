//! SWL Workflow Runtime
//!
//! Executes SWL workflows with agent delegation, tool calling, and state management.
//!
//! ## Guardrail Enforcement
//!
//! For workflows requiring guardrail enforcement, use `GuardedSwlRuntime` from the
//! `guarded` module:
//!
//! ```rust,ignore
//! use selfware::swl::runtime::{GuardedSwlRuntime, GuardedRuntimeBuilder};
//!
//! let runtime = GuardedRuntimeBuilder::new()
//!     .with_client(api_client)
//!     .with_verbose_guardrails()
//!     .build();
//!
//! runtime.register_guardrails(&doc).await;
//! let result = runtime.execute_workflow(&doc, "my_workflow", inputs).await?;
//! ```

pub mod guarded;

pub use guarded::{GuardedRuntimeBuilder, GuardedSwlRuntime};

use crate::api::{ApiClient, Message, ThinkingMode};
use crate::errors::Result;
use crate::observability::telemetry::{
    add_tokens_processed, increment_api_requests, record_state_transition, record_success,
    record_failure,
};
use crate::swl::parser::ast::{AgentDefinition, SwlDocument, WorkflowDefinition, WorkflowType};
use crate::swl::state::{StateBackendType, StateManager};
use crate::swl::types::schema::StateSchema;
use crate::orchestration::workflows::VarValue;
use crate::tools::ToolRegistry;
use crate::tool_parser::{parse_tool_calls, ParsedToolCall};
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
        let mut state_manager = StateManager::from_backend_type(backend_type, workflow_name).await?;
        
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
        if let Some(ref mut manager) = self.state_manager {
            *manager = std::mem::take(manager).with_schema(schema);
        }
        self
    }

    /// Get a state value as string (backward compatible)
    pub fn get(&self, key: &str) -> Option<String> {
        self.state.get(key).and_then(|v| {
            if let Some(s) = v.as_str() {
                Some(s.to_string())
            } else {
                // Convert other types to string representation
                Some(v.to_string())
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
        serde_json::to_string_pretty(&self.state)
            .map_err(|e| crate::errors::SelfwareError::Internal(
                format!("Failed to serialize state: {}", e)
            ))
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
        let workflow = doc
            .workflows
            .get(workflow_name)
            .ok_or_else(|| crate::errors::SelfwareError::Internal(format!(
                "Workflow '{}' not found in document",
                workflow_name
            )))?;

        // Execute based on workflow type
        let result = match workflow.workflow_type {
            WorkflowType::Sequential => {
                self.execute_sequential(doc, workflow).await
            }
            WorkflowType::Parallel => {
                self.execute_parallel(doc, workflow).await
            }
            WorkflowType::MapReduce => {
                self.execute_map_reduce(doc, workflow).await
            }
            WorkflowType::Conditional => {
                self.execute_conditional(doc, workflow).await
            }
        };

        // Persist state after workflow execution
        if let Err(e) = self.persist_state().await {
            warn!("Failed to persist workflow state: {}", e);
        }

        // Record workflow completion telemetry
        let duration_ms = workflow_start.elapsed().as_millis() as u64;
        self.last_workflow_duration_ms.store(duration_ms, Ordering::Relaxed);
        
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
            for (agent_name, _) in &doc.agents {
                println!("   [DRY-RUN] Would execute agent (parallel): {}", agent_name);
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
        for handle in handles {
            match handle.await {
                Ok(Ok((name, output))) => {
                    outputs.insert(name, output);
                }
                Ok(Err(e)) => {
                    warn!("Agent failed: {}", e);
                }
                Err(e) => {
                    warn!("Agent task panicked: {}", e);
                }
            }
        }

        let duration_ms = workflow_start.elapsed().as_millis() as u64;

        Ok(ExecutionResult {
            status: ExecutionStatus::Completed,
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

        // Reduce phase: if there's a reduce agent, run it
        if let Some(reduce_agent_name) = workflow.reduce.as_ref().and_then(|_reduce| {
            // Parse reduce code to get agent name
            // For now, simple heuristic: use the last agent as reducer
            doc.agents.keys().last().map(|s| s.to_string())
        }) {
            if let Some(agent) = doc.agents.get(&reduce_agent_name) {
                if !self.dry_run {
                    let _reduce_output = self.execute_agent(&reduce_agent_name, agent).await?;
                } else {
                    println!("   [DRY-RUN] Would execute reduce agent: {}", reduce_agent_name);
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
                println!("   [DRY-RUN] Would check condition with agent: {}", first_agent_name);
                "true".to_string()
            } else {
                self.execute_agent(first_agent_name, first_agent).await?
            };

            // Simple condition: if output contains "true", proceed
            if condition_result.to_lowercase().contains("true") {
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
    async fn execute_agent(
        &self,
        name: &str,
        agent: &AgentDefinition,
    ) -> Result<String> {
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

        let task_prompt = format!("{}", instruction);

        // Initialize conversation
        let mut messages = vec![
            Message::system(system_prompt),
            Message::user(task_prompt),
        ];

        // Tool execution loop
        let mut final_response = String::new();
        let mut iteration = 0;

        loop {
            iteration += 1;
            if iteration > self.max_tool_iterations {
                warn!("Agent {} reached maximum tool iterations ({})", name, self.max_tool_iterations);
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
                .chat(
                    messages.clone(),
                    tool_definitions,
                    ThinkingMode::Disabled,
                )
                .await?;

            let choice = response
                .choices
                .into_iter()
                .next()
                .ok_or_else(|| crate::errors::SelfwareError::Internal(
                    "Model returned no choices".to_string()
                ))?;

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

            // Check for native tool calls
            if let Some(tool_calls) = choice.message.tool_calls {
                if !tool_calls.is_empty() {
                    // Add assistant message with tool calls
                    messages.push(Message {
                        role: "assistant".to_string(),
                        content: crate::api::types::MessageContent::Text(
                            choice.message.content.text_all()
                        ),
                        reasoning_content: choice.message.reasoning_content.clone(),
                        tool_calls: Some(tool_calls.clone()),
                        tool_call_id: None,
                        name: None,
                    });

                    // Execute each tool call
                    for tool_call in &tool_calls {
                        let tool_result = self.execute_tool_call(tool_call, &available_tools).await;
                        
                        let result_content = match tool_result {
                            Ok(result) => result.to_string(),
                            Err(e) => format!("Error: {}", e),
                        };

                        // Add tool response message
                        messages.push(Message::tool(result_content, &tool_call.id));
                    }

                    // Continue the loop for the agent to process tool results
                    continue;
                }
            }

            // Parse text response for XML-style tool calls
            let text = choice.message.content.text_all();
            let parse_result = parse_tool_calls(&text);

            if !parse_result.tool_calls.is_empty() {
                // Add assistant message
                messages.push(Message::assistant(text.clone()));

                // Execute each parsed tool call
                for parsed_call in &parse_result.tool_calls {
                    let tool_result = self.execute_parsed_tool_call(parsed_call, &available_tools).await;
                    
                    let result_content = match tool_result {
                        Ok(result) => result.to_string(),
                        Err(e) => format!("Error: {}", e),
                    };

                    // Add tool response as a user message (since we don't have tool_call_id for parsed calls)
                    let tool_response = format!(
                        "Tool '{}' result:\n{}",
                        parsed_call.tool_name,
                        result_content
                    );
                    messages.push(Message::user(tool_response));
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
    fn build_tool_definitions(&self, tool_names: &[String]) -> Vec<crate::api::types::ToolDefinition> {
        tool_names
            .iter()
            .filter_map(|name| {
                self.tool_registry.get(name).map(|tool| {
                    crate::api::types::ToolDefinition {
                        def_type: "function".to_string(),
                        function: crate::api::types::FunctionDefinition {
                            name: tool.name().to_string(),
                            description: tool.description().to_string(),
                            parameters: tool.schema(),
                        },
                    }
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

        // Parse arguments
        let args: serde_json::Value = serde_json::from_str(&tool_call.function.arguments)?;
        
        // Execute the tool
        self.tool_registry.execute(&tool_call.function.name, args).await
    }

    /// Execute a parsed tool call from XML
    async fn execute_parsed_tool_call(
        &self,
        parsed_call: &ParsedToolCall,
        available_tools: &[String],
    ) -> anyhow::Result<serde_json::Value> {
        // Check if tool is in available tools list
        if !available_tools.contains(&parsed_call.tool_name) {
            anyhow::bail!(
                "Tool '{}' is not available to this agent. Available tools: {:?}",
                parsed_call.tool_name,
                available_tools
            );
        }

        // Execute the tool
        self.tool_registry.execute(&parsed_call.tool_name, parsed_call.arguments.clone()).await
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
        let mut summary = WorkflowTelemetry::default();
        
        summary.workflow_duration_ms = self.last_workflow_duration_ms.load(Ordering::Relaxed);
        
        // Aggregate per-agent metrics
        for event in &ctx.trace {
            let agent_metrics = summary.agent_metrics
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
        for (_, metrics) in &mut summary.agent_metrics {
            if metrics.total_calls > 0 {
                metrics.avg_latency_ms = metrics.total_latency_ms as f64 / metrics.total_calls as f64;
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
            agents: summary.agent_metrics.into_iter()
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
        
        serde_json::to_string_pretty(&export)
            .map_err(|e| crate::errors::SelfwareError::Internal(format!(
                "Failed to serialize telemetry: {}", e
            )))
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

/// Execution status
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionStatus {
    Completed,
    Failed,
    Cancelled,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_execution_context() {
        let mut ctx = ExecutionContext::new();
        ctx.set("key".to_string(), "value".to_string());
        assert_eq!(ctx.get("key"), Some(&"value".to_string()));
    }

    #[test]
    fn test_execution_status() {
        assert_eq!(ExecutionStatus::Completed, ExecutionStatus::Completed);
        assert_ne!(ExecutionStatus::Completed, ExecutionStatus::Failed);
    }

    #[test]
    fn test_agent_telemetry_default() {
        let telemetry = AgentTelemetry::default();
        assert_eq!(telemetry.total_calls, 0);
        assert_eq!(telemetry.total_tokens_in, 0);
        assert_eq!(telemetry.total_tokens_out, 0);
        assert_eq!(telemetry.total_latency_ms, 0);
        assert_eq!(telemetry.avg_latency_ms, 0.0);
    }

    #[test]
    fn test_workflow_telemetry_default() {
        let telemetry = WorkflowTelemetry::default();
        assert_eq!(telemetry.workflow_duration_ms, 0);
        assert!(telemetry.agent_metrics.is_empty());
        assert_eq!(telemetry.total_tokens, 0);
        assert_eq!(telemetry.total_api_calls, 0);
    }

    #[tokio::test]
    async fn test_telemetry_methods_with_dry_run() {
        let runtime = SwlRuntime::new_dry_run();
        
        // Test get_execution_trace returns empty initially
        let trace = runtime.get_execution_trace().await;
        assert!(trace.is_empty());
        
        // Test get_telemetry_summary returns default
        let summary = runtime.get_telemetry_summary().await;
        assert_eq!(summary.total_api_calls, 0);
        assert!(summary.agent_metrics.is_empty());
        
        // Test get_agent_trace returns empty for non-existent agent
        let agent_trace = runtime.get_agent_trace("test_agent").await;
        assert!(agent_trace.is_empty());
    }

    #[tokio::test]
    async fn test_clear_telemetry() {
        let runtime = SwlRuntime::new_dry_run();
        
        // Add a synthetic event to the trace
        {
            let mut ctx = runtime.context.lock().await;
            ctx.trace.push(ExecutionEvent {
                timestamp: std::time::Instant::now(),
                agent: "test".to_string(),
                action: "test".to_string(),
                duration_ms: 100,
                tokens_in: 10,
                tokens_out: 20,
            });
        }
        
        // Verify trace is not empty
        let trace = runtime.get_execution_trace().await;
        assert_eq!(trace.len(), 1);
        
        // Clear telemetry
        runtime.clear_telemetry().await;
        
        // Verify trace is empty
        let trace = runtime.get_execution_trace().await;
        assert!(trace.is_empty());
    }

    #[tokio::test]
    async fn test_telemetry_summary_aggregation() {
        let runtime = SwlRuntime::new_dry_run();
        
        // Add synthetic events to the trace
        {
            let mut ctx = runtime.context.lock().await;
            ctx.trace.push(ExecutionEvent {
                timestamp: std::time::Instant::now(),
                agent: "agent1".to_string(),
                action: "llm_call".to_string(),
                duration_ms: 100,
                tokens_in: 10,
                tokens_out: 20,
            });
            ctx.trace.push(ExecutionEvent {
                timestamp: std::time::Instant::now(),
                agent: "agent1".to_string(),
                action: "llm_call".to_string(),
                duration_ms: 200,
                tokens_in: 20,
                tokens_out: 30,
            });
            ctx.trace.push(ExecutionEvent {
                timestamp: std::time::Instant::now(),
                agent: "agent2".to_string(),
                action: "llm_call".to_string(),
                duration_ms: 150,
                tokens_in: 15,
                tokens_out: 25,
            });
        }
        
        // Get summary
        let summary = runtime.get_telemetry_summary().await;
        
        // Verify totals
        assert_eq!(summary.total_api_calls, 3);
        assert_eq!(summary.total_tokens, 120); // (10+20) + (20+30) + (15+25)
        
        // Verify agent1 metrics
        let agent1 = summary.agent_metrics.get("agent1").expect("agent1 should exist");
        assert_eq!(agent1.total_calls, 2);
        assert_eq!(agent1.total_tokens_in, 30);
        assert_eq!(agent1.total_tokens_out, 50);
        assert_eq!(agent1.total_latency_ms, 300);
        assert_eq!(agent1.avg_latency_ms, 150.0);
        
        // Verify agent2 metrics
        let agent2 = summary.agent_metrics.get("agent2").expect("agent2 should exist");
        assert_eq!(agent2.total_calls, 1);
        assert_eq!(agent2.total_tokens_in, 15);
        assert_eq!(agent2.total_tokens_out, 25);
        assert_eq!(agent2.total_latency_ms, 150);
        assert_eq!(agent2.avg_latency_ms, 150.0);
    }

    #[tokio::test]
    async fn test_get_agent_trace() {
        let runtime = SwlRuntime::new_dry_run();
        
        // Add synthetic events
        {
            let mut ctx = runtime.context.lock().await;
            ctx.trace.push(ExecutionEvent {
                timestamp: std::time::Instant::now(),
                agent: "agent1".to_string(),
                action: "llm_call".to_string(),
                duration_ms: 100,
                tokens_in: 10,
                tokens_out: 20,
            });
            ctx.trace.push(ExecutionEvent {
                timestamp: std::time::Instant::now(),
                agent: "agent2".to_string(),
                action: "llm_call".to_string(),
                duration_ms: 200,
                tokens_in: 20,
                tokens_out: 30,
            });
            ctx.trace.push(ExecutionEvent {
                timestamp: std::time::Instant::now(),
                agent: "agent1".to_string(),
                action: "llm_call".to_string(),
                duration_ms: 150,
                tokens_in: 15,
                tokens_out: 25,
            });
        }
        
        // Get agent1 trace
        let agent1_trace = runtime.get_agent_trace("agent1").await;
        assert_eq!(agent1_trace.len(), 2);
        assert_eq!(agent1_trace[0].duration_ms, 100);
        assert_eq!(agent1_trace[1].duration_ms, 150);
        
        // Get agent2 trace
        let agent2_trace = runtime.get_agent_trace("agent2").await;
        assert_eq!(agent2_trace.len(), 1);
        assert_eq!(agent2_trace[0].duration_ms, 200);
    }

    #[tokio::test]
    async fn test_export_telemetry_json() {
        let runtime = SwlRuntime::new_dry_run();
        
        // Add a synthetic event
        {
            let mut ctx = runtime.context.lock().await;
            ctx.trace.push(ExecutionEvent {
                timestamp: std::time::Instant::now(),
                agent: "test_agent".to_string(),
                action: "llm_call".to_string(),
                duration_ms: 100,
                tokens_in: 10,
                tokens_out: 20,
            });
        }
        
        // Export to JSON
        let json = runtime.export_telemetry_json().await;
        assert!(json.is_ok());
        
        let json_str = json.unwrap();
        assert!(json_str.contains("test_agent"));
        assert!(json_str.contains("total_tokens"));
        assert!(json_str.contains("total_api_calls"));
    }

    #[test]
    fn test_execution_event_creation() {
        let event = ExecutionEvent {
            timestamp: std::time::Instant::now(),
            agent: "test_agent".to_string(),
            action: "llm_call".to_string(),
            duration_ms: 150,
            tokens_in: 50,
            tokens_out: 100,
        };
        
        assert_eq!(event.agent, "test_agent");
        assert_eq!(event.action, "llm_call");
        assert_eq!(event.duration_ms, 150);
        assert_eq!(event.tokens_in, 50);
        assert_eq!(event.tokens_out, 100);
    }

    #[test]
    fn test_get_agent_tools() {
        let runtime = SwlRuntime::new_dry_run();
        
        // Agent with specific tools
        let agent = AgentDefinition {
            model: crate::swl::parser::ast::ModelSpec::Simple("test-model".to_string()),
            role: None,
            instruction: None,
            tools: vec!["file_read".to_string(), "file_write".to_string()],
            output_key: None,
            sub_agents: vec![],
        };
        
        let tools = runtime.get_agent_tools(&agent);
        assert_eq!(tools.len(), 2);
        assert!(tools.contains(&"file_read".to_string()));
        assert!(tools.contains(&"file_write".to_string()));
    }

    #[test]
    fn test_get_agent_tools_empty() {
        let runtime = SwlRuntime::new_dry_run();
        
        // Agent with no tools
        let agent = AgentDefinition {
            model: crate::swl::parser::ast::ModelSpec::Simple("test-model".to_string()),
            role: None,
            instruction: None,
            tools: vec![],
            output_key: None,
            sub_agents: vec![],
        };
        
        let tools = runtime.get_agent_tools(&agent);
        assert!(tools.is_empty());
    }

    #[test]
    fn test_build_system_prompt_with_tools() {
        let runtime = SwlRuntime::new_dry_run();
        
        let agent = AgentDefinition {
            model: crate::swl::parser::ast::ModelSpec::Simple("test-model".to_string()),
            role: Some("You are a coding assistant.".to_string()),
            instruction: None,
            tools: vec!["file_read".to_string()],
            output_key: None,
            sub_agents: vec![],
        };
        
        let tools = vec!["file_read".to_string()];
        let prompt = runtime.build_system_prompt(&agent, &tools);
        
        assert!(prompt.contains("You are a coding assistant."));
        assert!(prompt.contains("file_read"));
        assert!(prompt.contains("<tool>"));
        assert!(prompt.contains("</tool>"));
    }

    #[test]
    fn test_build_system_prompt_no_tools() {
        let runtime = SwlRuntime::new_dry_run();
        
        let agent = AgentDefinition {
            model: crate::swl::parser::ast::ModelSpec::Simple("test-model".to_string()),
            role: Some("You are a helpful assistant.".to_string()),
            instruction: None,
            tools: vec![],
            output_key: None,
            sub_agents: vec![],
        };
        
        let tools: Vec<String> = vec![];
        let prompt = runtime.build_system_prompt(&agent, &tools);
        
        assert_eq!(prompt, "You are a helpful assistant.");
    }

    #[test]
    fn test_build_tool_definitions() {
        let runtime = SwlRuntime::new_dry_run();
        
        let tool_names = vec!["file_read".to_string(), "shell_exec".to_string()];
        let definitions = runtime.build_tool_definitions(&tool_names);
        
        assert_eq!(definitions.len(), 2);
        
        let names: Vec<&str> = definitions.iter()
            .map(|d| d.function.name.as_str())
            .collect();
        assert!(names.contains(&"file_read"));
        assert!(names.contains(&"shell_exec"));
    }

    #[test]
    fn test_build_tool_definitions_empty() {
        let runtime = SwlRuntime::new_dry_run();
        
        let tool_names: Vec<String> = vec![];
        let definitions = runtime.build_tool_definitions(&tool_names);
        
        assert!(definitions.is_empty());
    }

    #[test]
    fn test_max_tool_iterations_config() {
        let runtime = SwlRuntime::new_dry_run()
            .with_max_tool_iterations(100);
        
        assert_eq!(runtime.max_tool_iterations, 100);
    }
}
