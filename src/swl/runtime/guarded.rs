//! Guarded SWL Runtime
//!
//! Extension of the SWL runtime with integrated guardrail enforcement.
//! Provides pre/post execution checkpoints for agents, tools, and workflows.

use super::{ExecutionContext, ExecutionEvent, ExecutionResult, ExecutionStatus, SwlRuntime};
use crate::api::{ApiClient, Message, ThinkingMode};
use crate::errors::{SafetyError, SelfwareError};
use crate::observability::telemetry::{
    add_tokens_processed, increment_api_requests, record_failure, record_state_transition,
    record_success,
};
use crate::orchestration::workflows::VarValue;
use crate::swl::guardrails::{
    GuardrailContext, GuardrailEnforcer, GuardrailSummary, GuardrailType,
};
use crate::swl::parser::ast::{AgentDefinition, SwlDocument, WorkflowDefinition, WorkflowType};
use crate::tool_parser::parse_tool_calls;
use crate::tools::ToolRegistry;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{debug, error, info, warn, Span};

/// SWL Runtime with integrated guardrail enforcement
pub struct GuardedSwlRuntime {
    /// Base runtime
    base: SwlRuntime,
    /// Guardrail enforcer
    enforcer: Arc<Mutex<GuardrailEnforcer>>,
    /// Workflow name for context
    current_workflow: Arc<Mutex<Option<String>>>,
}

impl GuardedSwlRuntime {
    /// Create a new guarded runtime with API client
    pub fn new(client: Arc<ApiClient>) -> Self {
        Self {
            base: SwlRuntime::new(client),
            enforcer: Arc::new(Mutex::new(GuardrailEnforcer::new())),
            current_workflow: Arc::new(Mutex::new(None)),
        }
    }

    /// Create a new guarded runtime with a custom tool registry
    pub fn with_tool_registry(client: Arc<ApiClient>, tool_registry: Arc<ToolRegistry>) -> Self {
        Self {
            base: SwlRuntime::with_tool_registry(client, tool_registry),
            enforcer: Arc::new(Mutex::new(GuardrailEnforcer::new())),
            current_workflow: Arc::new(Mutex::new(None)),
        }
    }

    /// Create a dry-run guarded runtime
    pub fn new_dry_run() -> Self {
        Self {
            base: SwlRuntime::new_dry_run(),
            enforcer: Arc::new(Mutex::new(GuardrailEnforcer::new())),
            current_workflow: Arc::new(Mutex::new(None)),
        }
    }

    /// Set the maximum number of tool iterations
    pub fn with_max_tool_iterations(self, max: usize) -> Self {
        Self {
            base: self.base.with_max_tool_iterations(max),
            enforcer: self.enforcer,
            current_workflow: self.current_workflow,
        }
    }

    /// Register guardrails from an SWL document
    pub async fn register_guardrails(&self, doc: &SwlDocument) {
        let mut enforcer = self.enforcer.lock().await;
        enforcer.register_guardrails(&doc.guardrails);
        info!(
            "Registered {} guardrails from document",
            doc.guardrails.len()
        );
    }

    /// Execute a workflow with guardrail enforcement
    pub async fn execute_workflow(
        &self,
        doc: &SwlDocument,
        workflow_name: &str,
        inputs: HashMap<String, VarValue>,
    ) -> crate::errors::Result<ExecutionResult> {
        info!("Executing guarded workflow: {}", workflow_name);

        // Register guardrails from document if not already registered
        self.register_guardrails(doc).await;

        // Set current workflow name
        {
            let mut wf = self.current_workflow.lock().await;
            *wf = Some(workflow_name.to_string());
        }

        // Register guardrails from document
        self.register_guardrails(doc).await;

        let workflow_start = std::time::Instant::now();
        record_state_transition("idle", "executing_workflow");

        // Build guardrail context for pre-workflow check
        let pre_context = self
            .build_guardrail_context(None, None, Some(&inputs))
            .await;

        // Pre-workflow guardrail check
        if let Some(blocking) = self
            .check_guardrails(GuardrailType::PreWorkflow, &pre_context)
            .await?
        {
            return Err(SelfwareError::Safety(SafetyError::BlockedCommand {
                command: format!("Workflow '{}' blocked by guardrails", workflow_name),
                reason: format!("{} violations found", blocking.len()),
            }));
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
            WorkflowType::Sequential => self.execute_sequential(doc, workflow, workflow_name).await,
            WorkflowType::Parallel => self.execute_parallel(doc, workflow, workflow_name).await,
            WorkflowType::MapReduce => self.execute_map_reduce(doc, workflow, workflow_name).await,
            WorkflowType::Conditional => {
                self.execute_conditional(doc, workflow, workflow_name).await
            }
        };

        // Post-workflow guardrail check
        let post_context = self.build_guardrail_context(None, None, None).await;
        let _post_summary = self
            .check_guardrails(GuardrailType::PostWorkflow, &post_context)
            .await?;

        // Record telemetry
        let duration_ms = workflow_start.elapsed().as_millis() as u64;

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

    /// Check guardrails and return blocking outcomes if any
    async fn check_guardrails(
        &self,
        guardrail_type: GuardrailType,
        context: &GuardrailContext,
    ) -> crate::errors::Result<Option<Vec<crate::swl::guardrails::GuardrailOutcome>>> {
        let enforcer = self.enforcer.lock().await;
        let summary = enforcer.check(guardrail_type, context).await?;

        if summary.should_block() {
            Ok(Some(
                summary.blocking_violations().into_iter().cloned().collect(),
            ))
        } else {
            Ok(None)
        }
    }

    /// Build guardrail context from current execution state
    async fn build_guardrail_context(
        &self,
        agent_name: Option<&str>,
        agent_output: Option<&str>,
        workflow_inputs: Option<&HashMap<String, VarValue>>,
    ) -> GuardrailContext {
        let ctx = self.base.get_context().await;
        let workflow = self.current_workflow.lock().await.clone();

        let mut context = GuardrailContext::new();

        // Add state
        for (key, value) in &ctx.state {
            context = context.with_state(key.clone(), value.clone());
        }

        // Add workflow name to state for telemetry
        if let Some(wf) = workflow {
            context = context.with_state("workflow_name", wf);
        }

        // Add agent info if present
        if let Some(name) = agent_name {
            context = context.with_current_agent(name);
        }

        // Add agent output if present
        if let (Some(name), Some(output)) = (agent_name, agent_output) {
            context = context.with_agent_output(name, output);
        }

        // Add workflow inputs if present
        if let Some(inputs) = workflow_inputs {
            for (key, value) in inputs {
                let json_value = serde_json::to_value(value).unwrap_or_default();
                context = context.with_workflow_input(key.clone(), json_value);
            }
        }

        context
    }

    /// Execute agents in sequence with guardrails
    async fn execute_sequential(
        &self,
        doc: &SwlDocument,
        _workflow: &WorkflowDefinition,
        workflow_name: &str,
    ) -> crate::errors::Result<ExecutionResult> {
        debug!("Executing sequential workflow with guardrails");
        let workflow_start = std::time::Instant::now();

        let mut outputs = HashMap::new();

        for (agent_name, agent) in &doc.agents {
            // Agent execution with guardrails is handled inside
            // execute_agent_with_guardrails (pre/post agent checks).
            let output = self
                .execute_agent_with_guardrails(agent_name, agent)
                .await?;
            outputs.insert(agent_name.clone(), output);
        }

        let duration_ms = workflow_start.elapsed().as_millis() as u64;

        Ok(ExecutionResult {
            status: ExecutionStatus::Completed,
            outputs,
            duration_ms,
        })
    }

    /// Execute agents in parallel with guardrails
    async fn execute_parallel(
        &self,
        doc: &SwlDocument,
        _workflow: &WorkflowDefinition,
        workflow_name: &str,
    ) -> crate::errors::Result<ExecutionResult> {
        debug!("Executing parallel workflow with guardrails");
        let workflow_start = std::time::Instant::now();

        // Pre-workflow guardrail check for all agents
        for agent_name in doc.agents.keys() {
            let pre_context = self
                .build_guardrail_context(Some(agent_name), None, None)
                .await;
            if let Some(blocking) = self
                .check_guardrails(GuardrailType::PreAgent, &pre_context)
                .await?
            {
                return Err(SelfwareError::Safety(SafetyError::BlockedCommand {
                    command: format!(
                        "Agent '{}' blocked by guardrails before execution",
                        agent_name
                    ),
                    reason: format!("{} violations found", blocking.len()),
                }));
            }
        }

        // Spawn all agents concurrently.  Pre/post agent guardrail checks
        // are handled inside execute_agent_with_guardrails.
        let mut handles = Vec::new();

        for (agent_name, agent) in &doc.agents {
            let agent_name = agent_name.clone();
            let agent = agent.clone();
            let runtime = self.clone();

            let handle = tokio::spawn(async move {
                let output = runtime
                    .execute_agent_with_guardrails(&agent_name, &agent)
                    .await?;

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
                    return Err(e);
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

    /// Execute map-reduce workflow with guardrails
    async fn execute_map_reduce(
        &self,
        doc: &SwlDocument,
        workflow: &WorkflowDefinition,
        workflow_name: &str,
    ) -> crate::errors::Result<ExecutionResult> {
        debug!("Executing map-reduce workflow with guardrails");
        let workflow_start = std::time::Instant::now();

        // Map phase: execute in parallel
        let map_result = self.execute_parallel(doc, workflow, workflow_name).await?;

        // Reduce phase
        if let Some(reduce_agent_name) = workflow
            .reduce
            .as_ref()
            .and_then(|_reduce| doc.agents.keys().last().map(|s| s.to_string()))
        {
            if let Some(agent) = doc.agents.get(&reduce_agent_name) {
                let _reduce_output = self
                    .execute_agent_with_guardrails(&reduce_agent_name, agent)
                    .await?;
            }
        }

        let duration_ms = workflow_start.elapsed().as_millis() as u64;

        Ok(ExecutionResult {
            status: ExecutionStatus::Completed,
            outputs: map_result.outputs,
            duration_ms,
        })
    }

    /// Execute conditional workflow with guardrails
    async fn execute_conditional(
        &self,
        doc: &SwlDocument,
        _workflow: &WorkflowDefinition,
        workflow_name: &str,
    ) -> crate::errors::Result<ExecutionResult> {
        debug!("Executing conditional workflow with guardrails");
        let workflow_start = std::time::Instant::now();

        if let Some((first_agent_name, first_agent)) = doc.agents.iter().next() {
            let condition_result = self
                .execute_agent_with_guardrails(first_agent_name, first_agent)
                .await?;

            if condition_result.to_lowercase().contains("true") {
                let mut outputs = HashMap::new();
                for (agent_name, agent) in doc.agents.iter().skip(1) {
                    let output = self
                        .execute_agent_with_guardrails(agent_name, agent)
                        .await?;
                    outputs.insert(agent_name.clone(), output);
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

    /// Execute a single agent with guardrails.
    ///
    /// Pre-agent and post-agent guardrail checks are run around the base
    /// runtime's agent execution.  If a pre-agent guardrail blocks, the
    /// agent is not executed and an error is returned.  If a post-agent
    /// guardrail blocks, the agent's output is rejected.
    async fn execute_agent_with_guardrails(
        &self,
        name: &str,
        agent: &AgentDefinition,
    ) -> crate::errors::Result<String> {
        // Pre-agent guardrail check
        let pre_context = self.build_guardrail_context(Some(name), None, None).await;
        if let Some(blocking) = self
            .check_guardrails(GuardrailType::PreAgent, &pre_context)
            .await?
        {
            return Err(SelfwareError::Safety(SafetyError::BlockedCommand {
                command: format!("Agent '{}' blocked by pre-execution guardrails", name),
                reason: format!("{} violations found", blocking.len()),
            }));
        }

        // Execute the agent via the base runtime
        let output = self.execute_agent_internal(name, agent).await?;

        // Post-agent guardrail check
        let post_context = self
            .build_guardrail_context(Some(name), Some(&output), None)
            .await;
        if let Some(blocking) = self
            .check_guardrails(GuardrailType::PostAgent, &post_context)
            .await?
        {
            return Err(SelfwareError::Safety(SafetyError::BlockedCommand {
                command: format!(
                    "Agent '{}' output blocked by post-execution guardrails",
                    name
                ),
                reason: format!("{} violations found", blocking.len()),
            }));
        }

        Ok(output)
    }

    /// Internal agent execution — delegates to the base runtime's real
    /// agent execution path (the same LLM call + tool-call loop used by
    /// the non-guarded runtime) so that workflows actually run agents
    /// instead of returning placeholder strings.
    async fn execute_agent_internal(
        &self,
        name: &str,
        agent: &AgentDefinition,
    ) -> crate::errors::Result<String> {
        info!("Executing agent with guardrails: {}", name);
        self.base.execute_agent(name, agent).await
    }

    /// Get telemetry summary
    pub async fn get_telemetry_summary(&self) -> super::WorkflowTelemetry {
        self.base.get_telemetry_summary().await
    }

    /// Export telemetry as JSON
    pub async fn export_telemetry_json(&self) -> crate::errors::Result<String> {
        self.base.export_telemetry_json().await
    }

    /// Get guardrail telemetry events
    pub async fn get_guardrail_telemetry(
        &self,
    ) -> Vec<crate::swl::guardrails::GuardrailTelemetryEvent> {
        let enforcer = self.enforcer.lock().await;
        enforcer.get_telemetry_events().await
    }

    /// Export guardrail telemetry as JSON
    pub async fn export_guardrail_telemetry_json(&self) -> crate::errors::Result<String> {
        let enforcer = self.enforcer.lock().await;
        enforcer.export_telemetry_json().await
    }
}

impl Clone for GuardedSwlRuntime {
    fn clone(&self) -> Self {
        // This is a simplified clone - in practice would need to properly
        // clone the Arc references
        Self {
            base: SwlRuntime::new_dry_run(),
            enforcer: Arc::clone(&self.enforcer),
            current_workflow: Arc::clone(&self.current_workflow),
        }
    }
}

/// Builder for creating guarded runtimes
pub struct GuardedRuntimeBuilder {
    client: Option<Arc<ApiClient>>,
    tool_registry: Option<Arc<ToolRegistry>>,
    enforcer: GuardrailEnforcer,
    dry_run: bool,
}

impl GuardedRuntimeBuilder {
    /// Create a new builder
    pub fn new() -> Self {
        Self {
            client: None,
            tool_registry: None,
            enforcer: GuardrailEnforcer::new(),
            dry_run: false,
        }
    }

    /// Set the API client
    pub fn with_client(mut self, client: Arc<ApiClient>) -> Self {
        self.client = Some(client);
        self
    }

    /// Set the tool registry
    pub fn with_tool_registry(mut self, registry: Arc<ToolRegistry>) -> Self {
        self.tool_registry = Some(registry);
        self
    }

    /// Set verbose mode for guardrails
    pub fn with_verbose_guardrails(mut self) -> Self {
        self.enforcer = GuardrailEnforcer::new_verbose();
        self
    }

    /// Enable dry run mode
    pub fn with_dry_run(mut self) -> Self {
        self.dry_run = true;
        self
    }

    /// Build the runtime
    pub fn build(self) -> GuardedSwlRuntime {
        if self.dry_run || self.client.is_none() {
            GuardedSwlRuntime::new_dry_run()
        } else if let Some(client) = self.client {
            if let Some(registry) = self.tool_registry {
                GuardedSwlRuntime::with_tool_registry(client, registry)
            } else {
                GuardedSwlRuntime::new(client)
            }
        } else {
            GuardedSwlRuntime::new_dry_run()
        }
    }
}

impl Default for GuardedRuntimeBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_guarded_runtime_creation() {
        let runtime = GuardedSwlRuntime::new_dry_run();
        // Should create without panicking
        let telemetry = runtime.get_guardrail_telemetry().await;
        assert!(telemetry.is_empty());
    }

    #[test]
    fn test_runtime_builder() {
        let runtime = GuardedRuntimeBuilder::new()
            .with_dry_run()
            .with_verbose_guardrails()
            .build();
        // Should create without panicking
    }
}
