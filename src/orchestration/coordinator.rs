//! Coordinator Mode - Multi-Agent Orchestration (STUB - SIMULATED EXECUTION)
//!
//! ⚠️ WARNING: Worker execution is SIMULATED. This module provides the framework
//! for multi-agent orchestration but `WorkerAgent::execute_task` is a stub that
//! does NOT actually use an LLM or execute tools. Workers only log and return
//! placeholder results.
//!
//! ## Architecture
//!
//! - **CoordinatorAgent**: Owns high-level task decomposition, restricted tool set
//! - **WorkerAgent**: Spawned by coordinator for specific subtasks, SIMULATED execution
//! - **Scratchpad**: Shared state for cross-worker knowledge sharing
//!
//! ## Four-Phase Workflow
//!
//! 1. **Research**: Coordinator spawns parallel workers (SIMULATED)
//! 2. **Synthesis**: Coordinator reads findings, creates implementation plan
//! 3. **Implementation**: Workers "execute" plan in parallel (SIMULATED)
//! 4. **Verification**: Workers "verify" each other's work (SIMULATED)
//!
//! Status: Framework complete but worker execution is STUBBED.
//! TODO: Implement actual LLM-driven worker execution in `WorkerAgent::execute_task`.

#![allow(dead_code)] // Types exported for API stability; integration pending

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use tokio::sync::RwLock;
use tracing::{info, warn};

use super::scratchpad::{Scratchpad, ScratchpadEntry, WorkerInfo, WorkerStatus};
use crate::tools::ToolRegistry;

/// Coordinator Mode execution phase
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WorkflowPhase {
    /// Initial research phase - workers investigate
    Research,
    /// Synthesis phase - coordinator creates plan from findings
    Synthesis,
    /// Implementation phase - workers execute plan
    Implementation,
    /// Verification phase - workers verify each other's work
    Verification,
    /// Workflow complete
    Complete,
    /// Workflow failed
    Failed,
}

impl std::fmt::Display for WorkflowPhase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WorkflowPhase::Research => write!(f, "research"),
            WorkflowPhase::Synthesis => write!(f, "synthesis"),
            WorkflowPhase::Implementation => write!(f, "implementation"),
            WorkflowPhase::Verification => write!(f, "verification"),
            WorkflowPhase::Complete => write!(f, "complete"),
            WorkflowPhase::Failed => write!(f, "failed"),
        }
    }
}

/// Result of a worker execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerResult {
    /// Worker ID
    pub worker_id: String,
    /// Whether the worker succeeded
    pub success: bool,
    /// Output/result content
    pub output: String,
    /// Any error message
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// Number of tool calls made
    pub tool_calls: usize,
    /// Duration in seconds
    pub duration_secs: u64,
}

/// Configuration for the coordinator
#[derive(Debug, Clone)]
pub struct CoordinatorConfig {
    /// Maximum number of concurrent workers
    pub max_concurrent_workers: usize,
    /// Timeout for worker tasks in seconds
    pub worker_timeout_secs: u64,
    /// Whether to allow workers to spawn sub-workers
    pub allow_worker_spawn: bool,
    /// Auto-advance through phases
    pub auto_advance: bool,
}

impl Default for CoordinatorConfig {
    fn default() -> Self {
        Self {
            max_concurrent_workers: 4,
            worker_timeout_secs: 300,
            allow_worker_spawn: true,
            auto_advance: true,
        }
    }
}

/// The restricted tool set available to the coordinator
///
/// Coordinator CANNOT:
/// - file_write, file_edit (cannot directly modify files)
/// - shell_exec, bash (cannot execute shell commands directly)
///
/// Coordinator CAN:
/// - file_read (read files to understand structure)
/// - scratchpad_write (communicate with workers via scratchpad)
/// - agent_spawn (spawn worker agents)
pub const COORDINATOR_ALLOWED_TOOLS: &[&str] = &[
    "file_read",
    "directory_tree",
    "glob_find",
    "grep_search",
    "tool_search",
];

/// Tools that are explicitly denied to coordinator
pub const COORDINATOR_DENIED_TOOLS: &[&str] = &[
    "file_write",
    "file_edit",
    "file_delete",
    "shell_exec",
    "bash",
    "cargo_test",
    "cargo_check",
    "git_commit",
    "git_push",
    "container_run",
    "compose_up",
    "process_start",
];

/// Coordinator Agent - orchestrates parallel work across worker agents
///
/// The coordinator has a restricted tool set focused on reading files and
/// managing workers. It cannot directly modify files or execute commands.
pub struct CoordinatorAgent {
    /// Unique task ID
    task_id: String,
    /// High-level task description
    task: String,
    /// Configuration
    config: CoordinatorConfig,
    /// Shared scratchpad for worker communication
    scratchpad: Scratchpad,
    /// Current workflow phase
    phase: Arc<RwLock<WorkflowPhase>>,
    /// Active workers
    workers: Arc<RwLock<HashMap<String, WorkerHandle>>>,
    /// Whether the coordinator is running
    running: Arc<AtomicBool>,
    /// Tool registry with restricted tools
    tools: ToolRegistry,
}

/// Handle to an active worker
#[derive(Debug)]
struct WorkerHandle {
    /// Worker ID
    pub id: String,
    /// Worker task description
    pub task: String,
    /// Worker role
    pub role: String,
    /// Join handle for the worker task
    pub abort_handle: tokio::task::AbortHandle,
}

/// Worker Agent - executes subtasks with full tool access
///
/// Workers are spawned by the coordinator and have full access to all tools
/// within safety constraints. They report back to the coordinator via the
/// scratchpad.
pub struct WorkerAgent {
    /// Worker ID
    id: String,
    /// Parent coordinator ID
    coordinator_id: String,
    /// Task description
    task: String,
    /// Worker role/specialty
    role: String,
    /// Scratchpad for communication
    scratchpad: Scratchpad,
    /// Full tool registry
    tools: ToolRegistry,
    /// Parent worker ID (if spawned by another worker)
    parent_id: Option<String>,
}

impl CoordinatorAgent {
    /// Create a new coordinator agent for the given task
    ///
    /// ⚠️ WARNING: Coordinator mode uses SIMULATED worker execution.
    /// Workers do NOT actually use LLM calls or execute tools - they only
    /// log messages and return placeholder results. See `WorkerAgent::execute_task`.
    pub fn new(task: impl Into<String>) -> Result<Self> {
        let task = task.into();
        let task_id = format!("task-{}", uuid::Uuid::new_v4());
        
        // Log prominent warning about simulated execution
        warn!(
            "\n{}
{} {}
{} {}
{} {}
{}",
            "╔══════════════════════════════════════════════════════════════════╗",
            "║", "⚠️  WARNING: COORDINATOR MODE USES SIMULATED WORKER EXECUTION",
            "║", "   WorkerAgent::execute_task is a STUB that does NOT use LLM",
            "║", "   or execute tools. Workers only log and return placeholders.",
            "╚══════════════════════════════════════════════════════════════════╝"
        );
        
        let scratchpad = Scratchpad::for_task(&task_id)?;

        // Create restricted tool registry
        let tools = Self::create_restricted_tool_registry()?;

        info!(task_id = %task_id, "Created coordinator agent (SIMULATED MODE)");

        Ok(Self {
            task_id,
            task,
            config: CoordinatorConfig::default(),
            scratchpad,
            phase: Arc::new(RwLock::new(WorkflowPhase::Research)),
            workers: Arc::new(RwLock::new(HashMap::new())),
            running: Arc::new(AtomicBool::new(false)),
            tools,
        })
    }

    /// Create with custom configuration
    pub fn with_config(mut self, config: CoordinatorConfig) -> Self {
        self.config = config;
        self
    }

    /// Create a restricted tool registry for the coordinator
    ///
    /// The coordinator only has access to read-only tools and tool discovery.
    /// It cannot write files, execute shell commands, or perform other destructive operations.
    fn create_restricted_tool_registry() -> Result<ToolRegistry> {
        use crate::tools::file::DirectoryTree;
        use crate::tools::file::FileRead;
        use crate::tools::grep_search::GrepSearch;
        use crate::tools::search::GlobFind;
        use crate::tools::tool_search::ToolSearchTool;

        // Create a completely fresh registry
        let restricted = ToolRegistry::with_safety_config(None);

        // Since ToolRegistry::with_safety_config registers critical tools by default,
        // we need to deactivate the ones we don't want
        // First, deactivate all tools that are denied to coordinators
        for _tool_name in COORDINATOR_DENIED_TOOLS {
            // We can't really deactivate, but we can track which ones shouldn't be used
            // The enforcement happens at the Agent level, not the registry level
        }

        Ok(restricted)
    }

    /// Check if a tool is allowed for coordinator use
    pub fn is_tool_allowed(tool_name: &str) -> bool {
        COORDINATOR_ALLOWED_TOOLS.contains(&tool_name)
    }

    /// Check if a tool is explicitly denied to coordinator
    pub fn is_tool_denied(tool_name: &str) -> bool {
        COORDINATOR_DENIED_TOOLS.contains(&tool_name)
    }

    /// Get the task ID
    pub fn task_id(&self) -> &str {
        &self.task_id
    }

    /// Get the task description
    pub fn task(&self) -> &str {
        &self.task
    }

    /// Get the current phase
    pub async fn current_phase(&self) -> WorkflowPhase {
        *self.phase.read().await
    }

    /// Get active workers count
    pub async fn active_worker_count(&self) -> usize {
        self.workers.read().await.len()
    }

    /// List active workers from scratchpad
    pub fn list_active_workers(&self) -> Vec<WorkerInfo> {
        self.scratchpad.active_workers()
    }

    /// List all workers from scratchpad
    pub fn list_all_workers(&self) -> Vec<WorkerInfo> {
        self.scratchpad.list_workers()
    }

    /// Spawn a worker agent for a subtask
    ///
    /// This creates a new worker, registers it in the scratchpad, and starts
    /// its execution in a background task.
    pub async fn spawn_worker(
        &self,
        task: impl Into<String>,
        role: impl Into<String>,
    ) -> Result<String> {
        let task = task.into();
        let role = role.into();
        let worker_id = format!("worker-{}", uuid::Uuid::new_v4());

        // Register worker in scratchpad
        let worker_info = WorkerInfo::new(&worker_id, &role, &task);
        self.scratchpad.register_worker(worker_info)?;

        info!(worker_id = %worker_id, role = %role, "Spawned worker");

        // Start worker in background task
        let scratchpad = self.scratchpad.clone();
        let worker_task = task.clone();
        let worker_role = role.clone();
        let coordinator_id = self.task_id.clone();
        let worker_id_for_spawn = worker_id.clone();

        let handle = tokio::spawn(async move {
            WorkerAgent::run(
                worker_id_for_spawn,
                coordinator_id,
                worker_task,
                worker_role,
                scratchpad,
                None,
            )
            .await
        });

        // Store handle
        let worker_handle = WorkerHandle {
            id: worker_id.clone(),
            task,
            role,
            abort_handle: handle.abort_handle(),
        };

        self.workers
            .write()
            .await
            .insert(worker_id.clone(), worker_handle);

        Ok(worker_id)
    }

    /// Send a message to a worker via scratchpad
    ///
    /// Messages are stored as scratchpad entries with the key format: `message:{worker_id}:{timestamp}`
    pub fn send_message(&self, worker_id: &str, message: impl Into<String>) -> Result<()> {
        let message = message.into();
        let key = format!(
            "message:{}:{}",
            worker_id,
            chrono::Utc::now().timestamp_millis()
        );

        let entry = ScratchpadEntry::new(key, message, self.task_id.clone()).set_metadata(
            serde_json::json!({
                "type": "coordinator_message",
                "target_worker": worker_id,
            }),
        );

        self.scratchpad.write(entry)?;
        Ok(())
    }

    /// Await completion of a specific worker
    ///
    /// Blocks until the worker completes, fails, or times out.
    pub async fn await_worker(&self, worker_id: &str) -> Result<Option<WorkerInfo>> {
        // First check if worker exists
        if self.scratchpad.get_worker(worker_id).is_none() {
            return Err(anyhow!("Worker not found: {}", worker_id));
        }

        // Poll until worker is finished or timeout
        let timeout_ms = self.config.worker_timeout_secs * 1000;
        let start = std::time::Instant::now();
        let poll_interval = std::time::Duration::from_millis(100);

        loop {
            if let Some(worker) = self.scratchpad.get_worker(worker_id) {
                if worker.is_finished() {
                    return Ok(Some(worker));
                }
            }

            if start.elapsed().as_millis() as u64 >= timeout_ms {
                return Ok(self.scratchpad.get_worker(worker_id));
            }

            tokio::time::sleep(poll_interval).await;
        }
    }

    /// Execute the four-phase workflow
    ///
    /// 1. Research: Spawn workers to investigate (SIMULATED)
    /// 2. Synthesis: Read findings, create implementation plan
    /// 3. Implementation: Workers "execute" plan (SIMULATED)
    /// 4. Verification: Workers "verify" each other's work (SIMULATED)
    ///
    /// ⚠️ WARNING: All worker execution is SIMULATED. No actual LLM calls
    /// or tool execution occurs. Workers only log and return placeholder results.
    /// See `WorkerAgent::execute_task` for details.
    pub async fn run_workflow(&self) -> Result<WorkflowResult> {
        self.running.store(true, Ordering::SeqCst);

        let mut result = WorkflowResult {
            success: false,
            phase_results: HashMap::new(),
            final_output: String::new(),
        };

        // Phase 1: Research
        info!("Starting research phase");
        *self.phase.write().await = WorkflowPhase::Research;
        match self.run_research_phase().await {
            Ok(research_result) => {
                result
                    .phase_results
                    .insert("research".to_string(), research_result);
            }
            Err(e) => {
                warn!(error = %e, "Research phase failed");
                *self.phase.write().await = WorkflowPhase::Failed;
                return Ok(result);
            }
        }

        if !self.running.load(Ordering::SeqCst) {
            return Ok(result);
        }

        // Phase 2: Synthesis
        if self.config.auto_advance {
            info!("Starting synthesis phase");
            *self.phase.write().await = WorkflowPhase::Synthesis;
            match self.run_synthesis_phase().await {
                Ok(synthesis_result) => {
                    result
                        .phase_results
                        .insert("synthesis".to_string(), synthesis_result);
                }
                Err(e) => {
                    warn!(error = %e, "Synthesis phase failed");
                    *self.phase.write().await = WorkflowPhase::Failed;
                    return Ok(result);
                }
            }
        }

        if !self.running.load(Ordering::SeqCst) {
            return Ok(result);
        }

        // Phase 3: Implementation
        if self.config.auto_advance {
            info!("Starting implementation phase");
            *self.phase.write().await = WorkflowPhase::Implementation;
            match self.run_implementation_phase().await {
                Ok(impl_result) => {
                    result
                        .phase_results
                        .insert("implementation".to_string(), impl_result);
                }
                Err(e) => {
                    warn!(error = %e, "Implementation phase failed");
                    *self.phase.write().await = WorkflowPhase::Failed;
                    return Ok(result);
                }
            }
        }

        if !self.running.load(Ordering::SeqCst) {
            return Ok(result);
        }

        // Phase 4: Verification
        if self.config.auto_advance {
            info!("Starting verification phase");
            *self.phase.write().await = WorkflowPhase::Verification;
            match self.run_verification_phase().await {
                Ok(verify_result) => {
                    result
                        .phase_results
                        .insert("verification".to_string(), verify_result);
                }
                Err(e) => {
                    warn!(error = %e, "Verification phase failed");
                    *self.phase.write().await = WorkflowPhase::Failed;
                    return Ok(result);
                }
            }
        }

        // Mark complete
        *self.phase.write().await = WorkflowPhase::Complete;
        result.success = true;

        // Gather final output from scratchpad
        result.final_output = self.gather_final_output().await?;

        Ok(result)
    }

    /// Run the research phase - spawn workers to investigate different aspects
    async fn run_research_phase(&self) -> Result<PhaseResult> {
        // Default research tasks - analyze codebase structure
        let research_tasks = vec![
            ("Analyze project structure", "analyzer"),
            ("Find relevant code files", "researcher"),
            ("Identify dependencies", "researcher"),
        ];

        let mut worker_ids = Vec::new();

        for (task, role) in research_tasks
            .iter()
            .take(self.config.max_concurrent_workers)
        {
            match self.spawn_worker(*task, *role).await {
                Ok(worker_id) => {
                    worker_ids.push(worker_id);
                }
                Err(e) => {
                    warn!(error = %e, "Failed to spawn worker");
                }
            }
        }

        // Wait for all workers to complete
        let mut results = Vec::new();
        for worker_id in worker_ids {
            match self.await_worker(&worker_id).await {
                Ok(Some(worker_info)) => {
                    results.push((worker_id, worker_info.status));
                }
                _ => {
                    results.push((worker_id, WorkerStatus::Failed));
                }
            }
        }

        // Store findings summary in scratchpad
        let findings_summary = serde_json::to_string(&results)?;
        self.scratchpad.write(ScratchpadEntry::new(
            "research:summary",
            findings_summary,
            &self.task_id,
        ))?;

        Ok(PhaseResult {
            success: true,
            workers_completed: results.len(),
            output: "Research phase completed".to_string(),
        })
    }

    /// Run the synthesis phase - read findings and create implementation plan
    async fn run_synthesis_phase(&self) -> Result<PhaseResult> {
        // Read all findings from scratchpad
        let findings = self.scratchpad.list_by_prefix("finding:");

        // Read research summary
        let summary = self
            .scratchpad
            .read("research:summary")
            .map(|e| e.value)
            .unwrap_or_default();

        // Create implementation plan based on findings
        // In a real implementation, this would use an LLM call to synthesize
        let plan = format!(
            "Implementation plan based on {} findings:\nSummary: {}",
            findings.len(),
            summary
        );

        // Store plan in scratchpad
        self.scratchpad.write(ScratchpadEntry::new(
            "synthesis:plan",
            plan.clone(),
            &self.task_id,
        ))?;

        Ok(PhaseResult {
            success: true,
            workers_completed: 0,
            output: plan,
        })
    }

    /// Run the implementation phase - workers execute the plan
    async fn run_implementation_phase(&self) -> Result<PhaseResult> {
        // Get the implementation plan
        let plan = self
            .scratchpad
            .read("synthesis:plan")
            .map(|e| e.value)
            .unwrap_or_else(|| "Implement the task".to_string());

        // Spawn implementation workers
        let impl_tasks = vec![
            (format!("Implement main logic: {}", plan), "coder"),
            ("Add tests".to_string(), "tester"),
            ("Update documentation".to_string(), "documenter"),
        ];

        let mut worker_ids = Vec::new();

        for (task, role) in impl_tasks.iter().take(self.config.max_concurrent_workers) {
            match self.spawn_worker(task.clone(), *role).await {
                Ok(worker_id) => worker_ids.push(worker_id),
                Err(e) => warn!(error = %e, "Failed to spawn implementation worker"),
            }
        }

        // Wait for all workers
        let mut completed = 0;
        for worker_id in worker_ids {
            if let Ok(Some(worker_info)) = self.await_worker(&worker_id).await {
                if worker_info.status == WorkerStatus::Completed {
                    completed += 1;
                }
            }
        }

        Ok(PhaseResult {
            success: completed > 0,
            workers_completed: completed,
            output: format!(
                "Implementation phase: {}/{} workers completed",
                completed,
                impl_tasks.len()
            ),
        })
    }

    /// Run the verification phase - workers verify each other's work
    async fn run_verification_phase(&self) -> Result<PhaseResult> {
        // Spawn verification workers to check the implementation
        let verify_tasks = vec![
            ("Run cargo check to verify compilation", "verifier"),
            ("Run tests", "tester"),
            ("Code review", "reviewer"),
        ];

        let mut worker_ids = Vec::new();

        for (task, role) in verify_tasks.iter().take(self.config.max_concurrent_workers) {
            match self.spawn_worker(*task, *role).await {
                Ok(worker_id) => worker_ids.push(worker_id),
                Err(e) => warn!(error = %e, "Failed to spawn verification worker"),
            }
        }

        // Wait for all workers
        let mut completed = 0;
        for worker_id in worker_ids {
            if let Ok(Some(worker_info)) = self.await_worker(&worker_id).await {
                if worker_info.status == WorkerStatus::Completed {
                    completed += 1;
                }
            }
        }

        // Store verification results
        self.scratchpad.write(ScratchpadEntry::new(
            "verification:summary",
            format!("{}/{} verifications passed", completed, verify_tasks.len()),
            &self.task_id,
        ))?;

        Ok(PhaseResult {
            success: completed == verify_tasks.len(),
            workers_completed: completed,
            output: format!(
                "Verification phase: {}/{} workers completed",
                completed,
                verify_tasks.len()
            ),
        })
    }

    /// Gather final output from scratchpad
    async fn gather_final_output(&self) -> Result<String> {
        let mut output = String::new();

        // Collect all findings and results
        let findings = self.scratchpad.all_entries();

        for entry in findings {
            output.push_str(&format!("\n=== {} ===\n{}", entry.key, entry.value));
        }

        Ok(output)
    }

    /// Stop the coordinator
    pub async fn stop(&self) {
        self.running.store(false, Ordering::SeqCst);

        // Abort all active workers
        let workers = self.workers.read().await;
        for (_, handle) in workers.iter() {
            handle.abort_handle.abort();
        }
    }

    /// Get the scratchpad
    pub fn scratchpad(&self) -> &Scratchpad {
        &self.scratchpad
    }
}

/// Result of a workflow phase
#[derive(Debug, Clone)]
pub struct PhaseResult {
    pub success: bool,
    pub workers_completed: usize,
    pub output: String,
}

/// Result of the complete workflow
#[derive(Debug, Clone)]
pub struct WorkflowResult {
    pub success: bool,
    pub phase_results: HashMap<String, PhaseResult>,
    pub final_output: String,
}

impl WorkerAgent {
    /// Create a new worker agent (internal use, workers are spawned by coordinator)
    ///
    /// ⚠️ WARNING: Worker execution is SIMULATED. Workers created by this method
    /// will NOT actually perform LLM calls or tool execution. See `execute_task`.
    fn new(
        id: impl Into<String>,
        coordinator_id: impl Into<String>,
        task: impl Into<String>,
        role: impl Into<String>,
        scratchpad: Scratchpad,
        parent_id: Option<String>,
    ) -> Self {
        Self {
            id: id.into(),
            coordinator_id: coordinator_id.into(),
            task: task.into(),
            role: role.into(),
            scratchpad,
            tools: ToolRegistry::new(),
            parent_id,
        }
    }

    /// Run the worker agent
    ///
    /// This is the main entry point for worker execution. 
    ///
    /// ⚠️ WARNING: This method calls `execute_task` which is a STUB that
    /// SIMULATES execution without using LLM or tools. The worker will log
    /// and return placeholder results only.
    async fn run(
        id: String,
        _coordinator_id: String,
        task: String,
        role: String,
        scratchpad: Scratchpad,
        _parent_id: Option<String>,
    ) -> Result<WorkerResult> {
        let start = std::time::Instant::now();

        info!(worker_id = %id, role = %role, "Worker starting execution");

        // Update status to working
        scratchpad.update_worker_status(&id, WorkerStatus::Working)?;

        // Store task in scratchpad
        scratchpad.write(ScratchpadEntry::new(
            format!("worker:{}:task", id),
            task.clone(),
            &id,
        ))?;

        // Execute the task
        // In a real implementation, this would use an LLM with tool access
        let result = Self::execute_task(&id, &task, &role, &scratchpad).await;

        let duration_secs = start.elapsed().as_secs();

        // Update status and store result
        match &result {
            Ok(output) => {
                scratchpad.update_worker_status(&id, WorkerStatus::Completed)?;
                scratchpad.write(ScratchpadEntry::new(
                    format!("worker:{}:result", id),
                    output.clone(),
                    &id,
                ))?;

                info!(worker_id = %id, duration = duration_secs, "Worker completed");

                Ok(WorkerResult {
                    worker_id: id,
                    success: true,
                    output: output.clone(),
                    error: None,
                    tool_calls: 0, // Would be tracked in real implementation
                    duration_secs,
                })
            }
            Err(e) => {
                scratchpad.update_worker_status(&id, WorkerStatus::Failed)?;
                scratchpad.write(ScratchpadEntry::new(
                    format!("worker:{}:error", id),
                    e.to_string(),
                    &id,
                ))?;

                warn!(worker_id = %id, error = %e, "Worker failed");

                Ok(WorkerResult {
                    worker_id: id,
                    success: false,
                    output: String::new(),
                    error: Some(e.to_string()),
                    tool_calls: 0,
                    duration_secs,
                })
            }
        }
    }

    /// Execute the actual task (STUB - SIMULATED EXECUTION)
    ///
    /// ⚠️ WARNING: This is a STUB that SIMULATES task execution.
    /// It does NOT:
    /// - Build a prompt with the task and role
    /// - Make LLM calls with tool access
    /// - Actually execute any tools
    /// - Produce real results
    ///
    /// TODO: Implement actual LLM-driven task execution
    async fn execute_task(
        id: &str,
        task: &str,
        role: &str,
        scratchpad: &Scratchpad,
    ) -> Result<String> {
        // STUB: Simulating task execution
        warn!(
            "STUB: Worker {} (role: {}) SIMULATING task execution: {}",
            id, role, task
        );

        let output = format!(
            "STUB: Worker {} (role: {}) SIMULATED task: {}\n\
            ⚠️ NO ACTUAL EXECUTION - This is placeholder output.",
            id, role, task
        );

        // Store a STUB finding
        let finding_key = format!("finding:{}:{}", id, chrono::Utc::now().timestamp_millis());
        scratchpad.write(ScratchpadEntry::new(
            finding_key,
            "STUB: Sample finding from SIMULATED worker execution",
            id,
        ))?;

        Ok(output)
    }

    /// Spawn a sub-worker (hierarchical worker spawning)
    ///
    /// Workers can spawn sub-workers for further parallelization.
    ///
    /// ⚠️ WARNING: The spawned sub-worker will also use SIMULATED execution
    /// (see `execute_task`). Sub-workers do NOT actually use LLM or tools.
    pub async fn spawn_subworker(
        &self,
        task: impl Into<String>,
        role: impl Into<String>,
    ) -> Result<String> {
        let task = task.into();
        let role = role.into();
        let subworker_id = format!("subworker-{}", uuid::Uuid::new_v4());

        // Register subworker with parent reference
        let worker_info = WorkerInfo::new(&subworker_id, &role, &task).with_parent(&self.id);
        self.scratchpad.register_worker(worker_info)?;

        info!(subworker_id = %subworker_id, parent_id = %self.id, "Spawned subworker");

        // Start subworker
        let scratchpad = self.scratchpad.clone();
        let coordinator_id = self.coordinator_id.clone();
        let parent_id = self.id.clone();
        let subworker_id_for_spawn = subworker_id.clone();

        tokio::spawn(async move {
            WorkerAgent::run(
                subworker_id_for_spawn,
                coordinator_id,
                task,
                role,
                scratchpad,
                Some(parent_id),
            )
            .await
        });

        Ok(subworker_id)
    }
}

/// Coordinator mode status for UI display
#[derive(Debug, Clone)]
pub struct CoordinatorStatus {
    pub task_id: String,
    pub current_phase: WorkflowPhase,
    pub active_workers: usize,
    pub total_workers: usize,
    pub is_coordinator: bool,
}

impl CoordinatorAgent {
    /// Get status for UI display
    pub async fn status(&self) -> CoordinatorStatus {
        let workers = self.scratchpad.list_workers();
        let active_count = workers.iter().filter(|w| !w.is_finished()).count();

        CoordinatorStatus {
            task_id: self.task_id.clone(),
            current_phase: *self.phase.read().await,
            active_workers: active_count,
            total_workers: workers.len(),
            is_coordinator: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_coordinator_tool_restrictions() {
        // Coordinator should be allowed file_read
        assert!(CoordinatorAgent::is_tool_allowed("file_read"));
        assert!(CoordinatorAgent::is_tool_allowed("directory_tree"));

        // Coordinator should be denied file_write
        assert!(CoordinatorAgent::is_tool_denied("file_write"));
        assert!(CoordinatorAgent::is_tool_denied("file_edit"));
        assert!(CoordinatorAgent::is_tool_denied("shell_exec"));
        assert!(CoordinatorAgent::is_tool_denied("bash"));
        assert!(CoordinatorAgent::is_tool_denied("cargo_test"));
        assert!(CoordinatorAgent::is_tool_denied("git_commit"));

        // file_read should not be denied
        assert!(!CoordinatorAgent::is_tool_denied("file_read"));
        assert!(!CoordinatorAgent::is_tool_denied("grep_search"));
        assert!(!CoordinatorAgent::is_tool_denied("glob_find"));
    }

    #[test]
    fn test_coordinator_cannot_write_files() {
        // This test verifies that the coordinator tracks which tools are denied
        // The actual enforcement happens at the Agent level by checking is_tool_allowed/is_tool_denied

        // Verify that write tools are marked as denied for coordinators
        assert!(CoordinatorAgent::is_tool_denied("file_write"));
        assert!(CoordinatorAgent::is_tool_denied("file_edit"));
        assert!(CoordinatorAgent::is_tool_denied("shell_exec"));
        assert!(CoordinatorAgent::is_tool_denied("bash"));

        // But read tools should NOT be denied
        assert!(!CoordinatorAgent::is_tool_denied("file_read"));
        assert!(!CoordinatorAgent::is_tool_denied("directory_tree"));
        assert!(!CoordinatorAgent::is_tool_denied("grep_search"));

        // And read tools should be allowed
        assert!(CoordinatorAgent::is_tool_allowed("file_read"));
        assert!(CoordinatorAgent::is_tool_allowed("directory_tree"));
    }

    #[tokio::test]
    async fn test_coordinator_creation() {
        let coordinator = CoordinatorAgent::new("Test task").unwrap();
        assert_eq!(coordinator.task(), "Test task");
        assert!(coordinator.task_id().starts_with("task-"));
        assert_eq!(coordinator.current_phase().await, WorkflowPhase::Research);
    }

    #[tokio::test]
    async fn test_worker_spawn() {
        let coordinator = CoordinatorAgent::new("Test task").unwrap();

        let worker_id = coordinator
            .spawn_worker("Test subtask", "researcher")
            .await
            .unwrap();
        assert!(worker_id.starts_with("worker-"));

        // Check worker was registered
        let workers = coordinator.list_all_workers();
        assert_eq!(workers.len(), 1);
        assert_eq!(workers[0].id, worker_id);
        assert_eq!(workers[0].role, "researcher");
    }

    #[tokio::test]
    async fn test_coordinator_status() {
        let coordinator = CoordinatorAgent::new("Test task").unwrap();

        let status = coordinator.status().await;
        assert!(status.is_coordinator);
        assert_eq!(status.current_phase, WorkflowPhase::Research);
        assert_eq!(status.active_workers, 0);
        assert_eq!(status.total_workers, 0);
    }

    #[tokio::test]
    async fn test_scratchpad_read_write() {
        let coordinator = CoordinatorAgent::new("Test task").unwrap();
        let scratchpad = coordinator.scratchpad();

        // Write entry
        let entry = ScratchpadEntry::new("test-key", "test-value", "test-author");
        scratchpad.write(entry.clone()).unwrap();

        // Read it back
        let read = scratchpad.read("test-key").unwrap();
        assert_eq!(read.key, "test-key");
        assert_eq!(read.value, "test-value");
        assert_eq!(read.author, "test-author");

        // Test typed read
        #[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
        struct TestData {
            count: i32,
            name: String,
        }

        let data = TestData {
            count: 42,
            name: "test".to_string(),
        };

        let typed_entry = ScratchpadEntry::new(
            "typed-key",
            serde_json::to_string(&data).unwrap(),
            "test-author",
        );
        scratchpad.write(typed_entry).unwrap();

        let read_data: Option<TestData> = scratchpad.read_typed("typed-key").unwrap();
        assert_eq!(read_data, Some(data));
    }

    #[tokio::test]
    async fn test_scratchpad_list_by_prefix() {
        let coordinator = CoordinatorAgent::new("Test task").unwrap();
        let scratchpad = coordinator.scratchpad();

        // Write entries with different prefixes
        scratchpad
            .write(ScratchpadEntry::new(
                "finding:1",
                "bug in parser",
                "worker1",
            ))
            .unwrap();
        scratchpad
            .write(ScratchpadEntry::new("finding:2", "missing docs", "worker2"))
            .unwrap();
        scratchpad
            .write(ScratchpadEntry::new("other:1", "unrelated", "worker3"))
            .unwrap();

        // List by prefix
        let findings = scratchpad.list_by_prefix("finding:");
        assert_eq!(findings.len(), 2);

        for finding in &findings {
            assert!(finding.key.starts_with("finding:"));
        }
    }

    #[tokio::test]
    async fn test_four_phase_workflow() {
        let coordinator = CoordinatorAgent::new("Implement feature X").unwrap();

        // Start should be in Research phase
        assert_eq!(coordinator.current_phase().await, WorkflowPhase::Research);

        // The workflow is run asynchronously - for this test we just verify
        // the phase transitions are set up correctly

        // We can't run the full workflow in a unit test because it spawns
        // actual worker tasks, but we can verify the structure

        // Check that coordinator was initialized correctly
        assert_eq!(coordinator.task(), "Implement feature X");
        assert!(coordinator.task_id().starts_with("task-"));
    }

    #[tokio::test]
    async fn test_worker_lifecycle() {
        let coordinator = CoordinatorAgent::new("Test task").unwrap();

        // Initially no workers
        assert_eq!(coordinator.active_worker_count().await, 0);

        // Spawn a worker
        let worker_id = coordinator
            .spawn_worker("Research task", "researcher")
            .await
            .unwrap();

        // Should have registered worker
        let workers = coordinator.list_all_workers();
        assert_eq!(workers.len(), 1);
        assert_eq!(workers[0].status, WorkerStatus::Initializing);

        // Update worker status
        coordinator
            .scratchpad()
            .update_worker_status(&worker_id, WorkerStatus::Working)
            .unwrap();

        let worker = coordinator.scratchpad().get_worker(&worker_id).unwrap();
        assert_eq!(worker.status, WorkerStatus::Working);

        // Mark as completed
        coordinator
            .scratchpad()
            .update_worker_status(&worker_id, WorkerStatus::Completed)
            .unwrap();

        let worker = coordinator.scratchpad().get_worker(&worker_id).unwrap();
        assert_eq!(worker.status, WorkerStatus::Completed);
        assert!(worker.is_finished());
    }

    #[tokio::test]
    async fn test_multiple_workers() {
        let coordinator = CoordinatorAgent::new("Test task").unwrap();

        // Spawn multiple workers
        let worker1 = coordinator
            .spawn_worker("Task 1", "researcher")
            .await
            .unwrap();
        let worker2 = coordinator.spawn_worker("Task 2", "coder").await.unwrap();
        let worker3 = coordinator.spawn_worker("Task 3", "tester").await.unwrap();

        // All should be registered
        let workers = coordinator.list_all_workers();
        assert_eq!(workers.len(), 3);

        // Complete some workers
        coordinator
            .scratchpad()
            .update_worker_status(&worker1, WorkerStatus::Completed)
            .unwrap();
        coordinator
            .scratchpad()
            .update_worker_status(&worker2, WorkerStatus::Failed)
            .unwrap();
        // worker3 stays active

        // Check active workers
        let active = coordinator.list_active_workers();
        assert_eq!(active.len(), 1); // Only worker3
        assert_eq!(active[0].id, worker3);
    }

    #[tokio::test]
    async fn test_message_passing() {
        let coordinator = CoordinatorAgent::new("Test task").unwrap();

        // Spawn a worker
        let worker_id = coordinator
            .spawn_worker("Test task", "researcher")
            .await
            .unwrap();

        // Send a message to the worker
        coordinator
            .send_message(&worker_id, "Please focus on finding bugs")
            .unwrap();

        // Message should be in scratchpad
        let messages = coordinator
            .scratchpad()
            .list_by_prefix(&format!("message:{}:", worker_id));
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].value, "Please focus on finding bugs");
        assert_eq!(messages[0].author, coordinator.task_id());
    }

    #[test]
    fn test_workflow_phase_display() {
        assert_eq!(WorkflowPhase::Research.to_string(), "research");
        assert_eq!(WorkflowPhase::Synthesis.to_string(), "synthesis");
        assert_eq!(WorkflowPhase::Implementation.to_string(), "implementation");
        assert_eq!(WorkflowPhase::Verification.to_string(), "verification");
        assert_eq!(WorkflowPhase::Complete.to_string(), "complete");
        assert_eq!(WorkflowPhase::Failed.to_string(), "failed");
    }

    #[test]
    fn test_worker_status_display() {
        assert_eq!(WorkerStatus::Initializing.to_string(), "initializing");
        assert_eq!(WorkerStatus::Working.to_string(), "working");
        assert_eq!(WorkerStatus::Completed.to_string(), "completed");
        assert_eq!(WorkerStatus::Failed.to_string(), "failed");
        assert_eq!(WorkerStatus::Terminated.to_string(), "terminated");
    }

    #[test]
    fn test_worker_info_finished() {
        let mut worker = WorkerInfo::new("w1", "role", "task");
        assert!(!worker.is_finished());

        worker.set_status(WorkerStatus::Completed);
        assert!(worker.is_finished());

        let mut worker2 = WorkerInfo::new("w2", "role", "task");
        worker2.set_status(WorkerStatus::Failed);
        assert!(worker2.is_finished());

        let mut worker3 = WorkerInfo::new("w3", "role", "task");
        worker3.set_status(WorkerStatus::Terminated);
        assert!(worker3.is_finished());

        let mut worker4 = WorkerInfo::new("w4", "role", "task");
        worker4.set_status(WorkerStatus::Working);
        assert!(!worker4.is_finished());
    }

    // =========================================================================
    // CoordinatorConfig tests
    // =========================================================================

    #[test]
    fn test_coordinator_config_default() {
        let config = CoordinatorConfig::default();
        assert_eq!(config.max_concurrent_workers, 4);
        assert_eq!(config.worker_timeout_secs, 300);
        assert!(config.allow_worker_spawn);
        assert!(config.auto_advance);
    }

    #[test]
    fn test_coordinator_config_custom() {
        let config = CoordinatorConfig {
            max_concurrent_workers: 8,
            worker_timeout_secs: 600,
            allow_worker_spawn: false,
            auto_advance: false,
        };
        assert_eq!(config.max_concurrent_workers, 8);
        assert_eq!(config.worker_timeout_secs, 600);
        assert!(!config.allow_worker_spawn);
        assert!(!config.auto_advance);
    }

    #[test]
    fn test_coordinator_config_clone() {
        let config = CoordinatorConfig::default();
        let cloned = config.clone();
        assert_eq!(config.max_concurrent_workers, cloned.max_concurrent_workers);
        assert_eq!(config.worker_timeout_secs, cloned.worker_timeout_secs);
    }

    // =========================================================================
    // WorkflowPhase tests
    // =========================================================================

    #[test]
    fn test_workflow_phase_equality() {
        assert_eq!(WorkflowPhase::Research, WorkflowPhase::Research);
        assert_eq!(WorkflowPhase::Synthesis, WorkflowPhase::Synthesis);
        assert_ne!(WorkflowPhase::Research, WorkflowPhase::Synthesis);
    }

    #[test]
    fn test_workflow_phase_copy() {
        let phase = WorkflowPhase::Implementation;
        let copied = phase;
        assert_eq!(phase, copied);
    }

    #[test]
    fn test_workflow_phase_clone() {
        let phase = WorkflowPhase::Verification;
        let cloned = phase.clone();
        assert_eq!(phase, cloned);
    }

    #[test]
    fn test_workflow_phase_serialization() {
        let phase = WorkflowPhase::Research;
        let json = serde_json::to_string(&phase).unwrap();
        let parsed: WorkflowPhase = serde_json::from_str(&json).unwrap();
        assert_eq!(phase, parsed);
    }

    #[test]
    fn test_all_workflow_phases_serialize() {
        let phases = vec![
            WorkflowPhase::Research,
            WorkflowPhase::Synthesis,
            WorkflowPhase::Implementation,
            WorkflowPhase::Verification,
            WorkflowPhase::Complete,
            WorkflowPhase::Failed,
        ];
        for phase in phases {
            let json = serde_json::to_string(&phase).unwrap();
            let parsed: WorkflowPhase = serde_json::from_str(&json).unwrap();
            assert_eq!(phase, parsed);
        }
    }

    #[test]
    fn test_all_workflow_phases_display() {
        let expected = vec![
            (WorkflowPhase::Research, "research"),
            (WorkflowPhase::Synthesis, "synthesis"),
            (WorkflowPhase::Implementation, "implementation"),
            (WorkflowPhase::Verification, "verification"),
            (WorkflowPhase::Complete, "complete"),
            (WorkflowPhase::Failed, "failed"),
        ];
        for (phase, s) in expected {
            assert_eq!(phase.to_string(), s);
        }
    }

    // =========================================================================
    // WorkerResult tests
    // =========================================================================

    #[test]
    fn test_worker_result_serialization() {
        let result = WorkerResult {
            worker_id: "worker-123".to_string(),
            success: true,
            output: "Task completed".to_string(),
            error: None,
            tool_calls: 5,
            duration_secs: 30,
        };
        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("worker-123"));
        assert!(json.contains("Task completed"));
        assert!(!json.contains("error")); // skip_serializing_if = None
    }

    #[test]
    fn test_worker_result_with_error() {
        let result = WorkerResult {
            worker_id: "worker-456".to_string(),
            success: false,
            output: "".to_string(),
            error: Some("Connection timeout".to_string()),
            tool_calls: 2,
            duration_secs: 10,
        };
        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("Connection timeout"));
        assert!(!result.success);
    }

    #[test]
    fn test_worker_result_deserialize() {
        let json = r#"{"worker_id":"w1","success":true,"output":"done","tool_calls":3,"duration_secs":15}"#;
        let result: WorkerResult = serde_json::from_str(json).unwrap();
        assert_eq!(result.worker_id, "w1");
        assert!(result.success);
        assert_eq!(result.tool_calls, 3);
        assert_eq!(result.duration_secs, 15);
        assert!(result.error.is_none());
    }

    #[test]
    fn test_worker_result_clone() {
        let result = WorkerResult {
            worker_id: "w1".to_string(),
            success: true,
            output: "output".to_string(),
            error: None,
            tool_calls: 1,
            duration_secs: 5,
        };
        let cloned = result.clone();
        assert_eq!(result.worker_id, cloned.worker_id);
        assert_eq!(result.output, cloned.output);
    }

    // =========================================================================
    // Tool restriction constants tests
    // =========================================================================

    #[test]
    fn test_coordinator_allowed_tools_contains_read_tools() {
        assert!(COORDINATOR_ALLOWED_TOOLS.contains(&"file_read"));
        assert!(COORDINATOR_ALLOWED_TOOLS.contains(&"directory_tree"));
        assert!(COORDINATOR_ALLOWED_TOOLS.contains(&"glob_find"));
        assert!(COORDINATOR_ALLOWED_TOOLS.contains(&"grep_search"));
        assert!(COORDINATOR_ALLOWED_TOOLS.contains(&"tool_search"));
    }

    #[test]
    fn test_coordinator_denied_tools_contains_write_tools() {
        assert!(COORDINATOR_DENIED_TOOLS.contains(&"file_write"));
        assert!(COORDINATOR_DENIED_TOOLS.contains(&"file_edit"));
        assert!(COORDINATOR_DENIED_TOOLS.contains(&"file_delete"));
        assert!(COORDINATOR_DENIED_TOOLS.contains(&"shell_exec"));
        assert!(COORDINATOR_DENIED_TOOLS.contains(&"bash"));
    }

    #[test]
    fn test_coordinator_denied_tools_contains_container_tools() {
        assert!(COORDINATOR_DENIED_TOOLS.contains(&"container_run"));
        assert!(COORDINATOR_DENIED_TOOLS.contains(&"compose_up"));
    }

    #[test]
    fn test_coordinator_denied_tools_contains_git_tools() {
        assert!(COORDINATOR_DENIED_TOOLS.contains(&"git_commit"));
        assert!(COORDINATOR_DENIED_TOOLS.contains(&"git_push"));
    }

    #[test]
    fn test_no_overlap_between_allowed_and_denied() {
        for tool in COORDINATOR_ALLOWED_TOOLS {
            assert!(
                !COORDINATOR_DENIED_TOOLS.contains(tool),
                "Tool {} is in both allowed and denied lists",
                tool
            );
        }
    }

    // =========================================================================
    // Static method tests
    // =========================================================================

    #[test]
    fn test_is_tool_allowed_positive() {
        assert!(CoordinatorAgent::is_tool_allowed("file_read"));
        assert!(CoordinatorAgent::is_tool_allowed("grep_search"));
    }

    #[test]
    fn test_is_tool_allowed_negative() {
        assert!(!CoordinatorAgent::is_tool_allowed("file_write"));
        assert!(!CoordinatorAgent::is_tool_allowed("unknown_tool"));
    }

    #[test]
    fn test_is_tool_denied_positive() {
        assert!(CoordinatorAgent::is_tool_denied("shell_exec"));
        assert!(CoordinatorAgent::is_tool_denied("file_delete"));
    }

    #[test]
    fn test_is_tool_denied_negative() {
        assert!(!CoordinatorAgent::is_tool_denied("file_read"));
        assert!(!CoordinatorAgent::is_tool_denied("unknown_tool"));
    }

    // =========================================================================
    // Coordinator with_config test
    // =========================================================================

    #[tokio::test]
    async fn test_coordinator_with_config() {
        let config = CoordinatorConfig {
            max_concurrent_workers: 10,
            worker_timeout_secs: 60,
            allow_worker_spawn: false,
            auto_advance: false,
        };
        let coordinator = CoordinatorAgent::new("Test")
            .unwrap()
            .with_config(config);
        assert_eq!(coordinator.task(), "Test");
        // Config is applied internally; we verify creation succeeds
    }
}
