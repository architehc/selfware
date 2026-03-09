use anyhow::Result;
use std::collections::{HashSet, VecDeque};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::Instant;
use tracing::{info, warn};

use crate::analyzer::ErrorAnalyzer;
use crate::api::types::{Message, ToolCall};
use crate::api::{ApiClient, StreamChunk, ThinkingMode};
use crate::checkpoint::{CheckpointManager, TaskCheckpoint};
use crate::cognitive::self_improvement::{Outcome, SelfImprovementEngine};
use crate::cognitive::{CognitiveState, CyclePhase};
use crate::concurrency::ConcurrencyGovernor;
use crate::config::Config;
use crate::hooks::HookRegistry;
use crate::memory::AgentMemory;
use crate::output;
use crate::safety::SafetyChecker;
#[cfg(feature = "resilience")]
use crate::self_healing::{SelfHealingConfig, SelfHealingEngine};
use crate::session::chat_store::ChatStore;
use crate::session::edit_history::EditHistory;
use crate::telemetry::{enter_agent_step, record_state_transition};
use crate::tools::file::init_safety_config;
use crate::tools::ToolRegistry;
use crate::verification::{VerificationConfig, VerificationGate};

mod checkpointing;
pub mod context;
mod context_management;
mod execution;
mod interactive;
pub mod last_tool;
mod learning;
pub mod loop_control;
pub mod planning;
mod streaming;
mod task_runner;
pub mod tui_events;

use crate::errors::is_confirmation_error;
use context::ContextCompressor;
use loop_control::{AgentLoop, AgentState};
use planning::Planner;
use tui_events::{AgentEvent, EventEmitter, NoopEmitter};

/// Upper bound for queued interactive messages to avoid unbounded memory growth.
pub(crate) const MAX_PENDING_MESSAGES: usize = 100;

/// Detected project type for adapting verification instructions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ProjectType {
    Rust,
    Node,
    Python,
    Go,
    Generic,
}

/// Detect the project type from marker files in the working directory.
fn detect_project_type() -> ProjectType {
    if std::path::Path::new("Cargo.toml").exists() {
        ProjectType::Rust
    } else if std::path::Path::new("package.json").exists() {
        ProjectType::Node
    } else if std::path::Path::new("pyproject.toml").exists()
        || std::path::Path::new("setup.py").exists()
        || std::path::Path::new("requirements.txt").exists()
    {
        ProjectType::Python
    } else if std::path::Path::new("go.mod").exists() {
        ProjectType::Go
    } else {
        ProjectType::Generic
    }
}

/// Return (verify_step, test_step, completion_rule) for the detected project type.
fn verification_instructions(pt: ProjectType) -> (&'static str, &'static str, &'static str) {
    match pt {
        ProjectType::Rust => (
            "3. VERIFY: Run cargo_check IMMEDIATELY after every file change",
            "5. TEST: Run cargo_test when implementation is complete",
            "- NEVER declare complete without a successful cargo_check",
        ),
        ProjectType::Node => (
            "3. VERIFY: Check for syntax errors after changes. Run npm test or the project's test script if available",
            "5. TEST: Run the project's test command when implementation is complete",
            "- Verify your changes work before declaring complete",
        ),
        ProjectType::Python => (
            "3. VERIFY: Check for syntax errors after changes. Run pytest or the project's test command if available",
            "5. TEST: Run pytest or the project's test command when implementation is complete",
            "- Verify your changes work before declaring complete",
        ),
        ProjectType::Go => (
            "3. VERIFY: Run go build after every file change",
            "5. TEST: Run go test when implementation is complete",
            "- NEVER declare complete without a successful go build",
        ),
        ProjectType::Generic => (
            "3. VERIFY: Test your changes using appropriate tools for the project type",
            "5. TEST: Verify the output works correctly when implementation is complete",
            "- Verify your changes work before declaring complete",
        ),
    }
}

/// Core agent that orchestrates LLM reasoning with tool execution.
///
/// The agent maintains conversation state, manages tool calls through a safety
/// checker, supports checkpointing for task resumption, and implements an
/// observe-orient-decide-act cognitive loop.
pub struct Agent {
    client: ApiClient,
    tools: ToolRegistry,
    memory: AgentMemory,
    safety: SafetyChecker,
    config: Config,
    loop_control: AgentLoop,
    messages: Vec<Message>,
    compressor: ContextCompressor,
    checkpoint_manager: Option<CheckpointManager>,
    pub current_checkpoint: Option<TaskCheckpoint>,
    /// Cognitive state for PDVR cycle and working memory
    cognitive_state: CognitiveState,
    /// Runtime learner that adapts prompt/tool/error strategy from outcomes
    self_improvement: SelfImprovementEngine,
    /// Current task description used as learning context for tool/error feedback
    current_task_context: String,
    /// Verification gate for automatic code validation
    verification_gate: VerificationGate,
    /// Error analyzer for intelligent error suggestions
    error_analyzer: ErrorAnalyzer,
    /// Files loaded into context for reload functionality
    context_files: Vec<String>,
    /// Files modified since last loaded into context (need refresh)
    stale_files: HashSet<String>,
    /// Last time a checkpoint was persisted to disk
    last_checkpoint_persisted_at: Instant,
    /// Tool call count at last persisted checkpoint
    last_checkpoint_tool_calls: usize,
    /// Whether at least one checkpoint has been persisted in this session
    checkpoint_persisted_once: bool,
    /// Event emitter for real-time updates (TUI or other)
    #[allow(dead_code)]
    events: Arc<dyn EventEmitter>,
    /// Edit history for undo support
    edit_history: EditHistory,
    /// Last assistant response content (for /copy command)
    last_assistant_response: String,
    /// Chat session store for save/resume/list/delete
    chat_store: ChatStore,
    /// Cancellation token set by Ctrl+C while a task is running
    cancelled: Arc<AtomicBool>,
    /// Messages queued for sequential execution
    pending_messages: VecDeque<String>,
    /// Maximum total estimated tokens for the message history.
    /// When exceeded, oldest non-system messages are removed.
    max_context_tokens: usize,
    /// Self-healing engine for automatic recovery attempts
    #[cfg(feature = "resilience")]
    self_healing: SelfHealingEngine,
    /// Recent tool call signatures for repetition detection (name, args_hash)
    recent_tool_calls: VecDeque<(String, u64)>,
    /// Hook registry for event-driven automation
    hook_registry: HookRegistry,
    /// Plan mode: propose tool calls without executing them
    plan_mode: bool,
    /// Audit logger for JSONL tool execution logging
    audit_logger: Option<crate::safety::audit::AuditLogger>,
    /// Permission store for pre-authorized tool grants
    permission_store: crate::safety::permissions::PermissionStore,
    /// Tool result cache for read-only operations
    tool_cache: crate::session::cache::ToolCache,
    /// Local-first coordinator for response caching and offline support
    local_first: crate::session::local_first::LocalFirstCoordinator,
    /// Concurrency governor for limiting concurrent streams and tool executions
    governor: ConcurrencyGovernor,
    /// Pause flag for the ESC listener — set when a confirmation prompt needs stdin
    esc_paused: Arc<AtomicBool>,
}

impl Agent {
    pub async fn new(config: Config) -> Result<Self> {
        let client = ApiClient::new(&config)?;
        let mut tools = ToolRegistry::new();
        tools.register(crate::tools::fim::FileFimEdit::new(std::sync::Arc::new(
            client.clone(),
        )));
        let memory = AgentMemory::new(&config)?;
        let safety = SafetyChecker::new(&config.safety);
        // Publish the user-loaded safety config so file tools honour allowed_paths etc.
        init_safety_config(&config.safety);
        let loop_control = AgentLoop::new(config.agent.max_iterations);
        let compressor = ContextCompressor::new(config.max_tokens);

        // Initialize cognitive state and load global episodic memory if available
        let mut cognitive_state = CognitiveState::new();
        let global_memory_path = dirs::data_local_dir()
            .unwrap_or_else(|| std::path::PathBuf::from("."))
            .join("selfware")
            .join("global_episodic_memory.json");

        if let Ok(content) = tokio::fs::read_to_string(&global_memory_path).await {
            if let Ok(loaded_memory) =
                serde_json::from_str::<crate::cognitive::EpisodicMemory>(&content)
            {
                cognitive_state.episodic_memory = loaded_memory;
                info!("Loaded global episodic memory for recursive self-improvement");
            }
        }

        // Load persisted self-improvement engine state if available
        let improvement_engine_path = dirs::data_local_dir()
            .unwrap_or_else(|| std::path::PathBuf::from("."))
            .join("selfware")
            .join("improvement_engine.json");

        let self_improvement = if improvement_engine_path.exists() {
            match SelfImprovementEngine::load(&improvement_engine_path) {
                Ok(engine) => {
                    info!("Loaded persisted self-improvement engine state");
                    engine
                }
                Err(e) => {
                    warn!(
                        "Failed to load improvement engine state: {}, starting fresh",
                        e
                    );
                    SelfImprovementEngine::new()
                }
            }
        } else {
            SelfImprovementEngine::new()
        };

        // Detect project type for verification instructions
        let project_type = detect_project_type();
        let (verify_step, test_step, completion_rule) = verification_instructions(project_type);
        info!("Detected project type: {:?}", project_type);

        // Choose between native function calling or XML-based tool parsing
        let mut system_prompt = if config.agent.native_function_calling {
            // Native function calling: simple prompt, tools passed via API
            info!("Using native function calling mode");
            format!(
                r#"You are Selfware, an expert software engineering AI assistant.

You have access to tools for file operations, git, shell commands, and more.

## MANDATORY WORKFLOW
1. PLAN: Understand what needs to change — read relevant files first
2. IMPLEMENT: Make code changes using file_edit or file_write
{}
4. FIX: If verification fails, fix errors before proceeding
{}

## CRITICAL RULES
- NEVER skip verification after file_edit or file_write
{}
- When editing files, include 3-5 lines of context for unique matches
- You have a large budget. Do NOT rush. Be thorough and methodical.
- When the task is complete, respond with a summary of what was done."#,
                verify_step, test_step, completion_rule
            )
        } else {
            // XML-based: embed tools in system prompt
            // This works with backends that don't support native function calling
            let tool_descriptions = tools
                .list()
                .iter()
                .map(|t| {
                    format!(
                        r#"<tool name="{}">
  <description>{}</description>
  <parameters>{}</parameters>
</tool>"#,
                        t.name(),
                        t.description(),
                        t.schema()
                    )
                })
                .collect::<Vec<_>>()
                .join("\n");

            format!(
                r#"You are Selfware, an expert software engineering AI assistant with access to tools.

Available tools:
{}

## Tool Format (MUST follow exactly)

To call a tool, use this EXACT XML structure:

<tool>
<name>TOOL_NAME</name>
<arguments>JSON_OBJECT</arguments>
</tool>

### Correct examples:

<tool>
<name>file_read</name>
<arguments>{{"path": "./src/main.rs"}}</arguments>
</tool>

<tool>
<name>directory_tree</name>
<arguments>{{"path": "./src", "max_depth": 3}}</arguments>
</tool>

<tool>
<name>shell_exec</name>
<arguments>{{"command": "cargo build"}}</arguments>
</tool>

### WRONG formats (DO NOT USE):
- <function>tool_name</function> - WRONG
- <function=tool_name> - WRONG
- <name=tool_name> - WRONG
- Any format other than <name>tool_name</name> - WRONG

## MANDATORY WORKFLOW
1. PLAN: Understand what needs to change — read relevant files first
2. IMPLEMENT: Make code changes using file_edit or file_write
{}
4. FIX: If verification fails, fix errors before proceeding
{}

## CRITICAL RULES
- Use <name>TOOL_NAME</name> - never <function>
- Arguments must be valid JSON inside <arguments>...</arguments>
- Each <tool>...</tool> block is executed separately
- Wait for tool results before proceeding
- NEVER skip verification after file_edit or file_write
{}
- You have a large budget. Do NOT rush. Be thorough and methodical.
- When done, respond with plain text only (no tool tags)"#,
                tool_descriptions, verify_step, test_step, completion_rule
            )
        };

        // Inject past lessons to avoid repeating mistakes
        let recent_lessons = cognitive_state.episodic_memory.recent_lessons(10);
        if !recent_lessons.is_empty() {
            system_prompt.push_str("\n\n## Global Lessons Learned\nDo not repeat past mistakes. Consider these lessons:\n");
            for lesson in recent_lessons {
                system_prompt.push_str(&format!("- {}\n", lesson));
            }
        }
        if let Some(tournament) = self_improvement.evolve_prompt(&system_prompt, "system_prompt") {
            if tournament.winner_prompt != system_prompt {
                info!(
                    "Applied evolved system prompt variant '{}' (predicted quality {:.2})",
                    tournament.winner_strategy, tournament.winner_score
                );
                system_prompt = tournament.winner_prompt;
            }
        }

        let messages = vec![Message::system(system_prompt)];

        // Initialize checkpoint manager if configured
        let checkpoint_manager = CheckpointManager::default_path().ok();

        // Initialize verification gate with project root
        let project_root = std::env::current_dir().unwrap_or_else(|_| ".".into());
        let verification_gate = VerificationGate::new(&project_root, VerificationConfig::fast());

        // Initialize error analyzer
        let error_analyzer = ErrorAnalyzer::new();

        #[cfg(feature = "resilience")]
        let self_healing = SelfHealingEngine::new(SelfHealingConfig {
            enabled: config.continuous_work.auto_recovery,
            max_healing_attempts: config.continuous_work.max_recovery_attempts,
            checkpoint_interval_secs: config.continuous_work.checkpoint_interval_secs,
            ..Default::default()
        });

        let edit_history = EditHistory::new();
        let chat_store = ChatStore::new().unwrap_or_else(|_| ChatStore::fallback());

        // Connect to MCP servers and register their tools
        if !config.mcp.servers.is_empty() {
            info!(
                "Connecting to {} MCP server(s)...",
                config.mcp.servers.len()
            );
            for server_config in &config.mcp.servers {
                match crate::mcp::McpClient::connect(server_config).await {
                    Ok(client) => {
                        let client = std::sync::Arc::new(client);
                        match crate::mcp::discover_tools(&client).await {
                            Ok(mcp_tools) => {
                                let count = mcp_tools.len();
                                for tool in mcp_tools {
                                    tools.register(tool);
                                }
                                info!(
                                    "Registered {} tool(s) from MCP server '{}'",
                                    count, server_config.name
                                );
                            }
                            Err(e) => {
                                warn!(
                                    "Failed to discover tools from MCP server '{}': {}",
                                    server_config.name, e
                                );
                            }
                        }
                    }
                    Err(e) => {
                        warn!(
                            "Failed to connect to MCP server '{}': {}",
                            server_config.name, e
                        );
                    }
                }
            }
        }

        // Initialize hook registry from configuration
        let hook_registry = HookRegistry::from_config(&config.hooks);
        if !hook_registry.is_empty() {
            info!("Loaded {} hook(s) from configuration", hook_registry.len());
        }

        let plan_mode = config.plan_mode;

        // Initialize audit logger (writes JSONL events to ~/.selfware/audit/)
        let session_id = uuid::Uuid::new_v4().to_string();
        let audit_logger = crate::safety::audit::AuditLogger::new(&session_id);
        if let Some(ref logger) = audit_logger {
            logger.log_session_start();
        }

        // Initialize permission store from config grants
        let permission_store =
            crate::safety::permissions::PermissionStore::from_config(&config.safety.permissions);

        // Initialize session encryption from OS keychain (if available)
        if let Ok(Some(password)) =
            crate::session::encryption::EncryptionManager::load_from_keychain()
        {
            let _ = crate::session::encryption::EncryptionManager::init(&password);
        }

        info!("Agent initialized with cognitive state, verification gate, and error analyzer");

        Ok(Self {
            client,
            tools,
            memory,
            safety,
            config,
            loop_control,
            messages,
            compressor,
            checkpoint_manager,
            current_checkpoint: None,
            cognitive_state,
            self_improvement,
            current_task_context: String::new(),
            verification_gate,
            error_analyzer,
            context_files: Vec::new(),
            stale_files: HashSet::new(),
            last_checkpoint_persisted_at: Instant::now(),
            last_checkpoint_tool_calls: 0,
            checkpoint_persisted_once: false,
            events: Arc::new(NoopEmitter),
            edit_history,
            last_assistant_response: String::new(),
            chat_store,
            cancelled: Arc::new(AtomicBool::new(false)),
            pending_messages: VecDeque::new(),
            max_context_tokens: 100_000,
            #[cfg(feature = "resilience")]
            self_healing,
            recent_tool_calls: VecDeque::new(),
            hook_registry,
            plan_mode,
            audit_logger,
            permission_store,
            tool_cache: crate::session::cache::ToolCache::new(),
            local_first: crate::session::local_first::LocalFirstCoordinator::new(),
            governor: ConcurrencyGovernor::with_defaults(),
            esc_paused: Arc::new(AtomicBool::new(false)),
        })
    }

    /// Set the TUI event sender for real-time updates
    #[cfg(feature = "tui")]
    pub fn with_event_sender(
        mut self,
        tx: std::sync::mpsc::Sender<crate::ui::tui::TuiEvent>,
    ) -> Self {
        self.events = Arc::new(tui_events::TuiEmitter::new(tx));
        self
    }

    /// Emit an event to the TUI / event listener (no-op when no emitter is configured).
    fn emit_event(&self, event: AgentEvent) {
        self.events.emit(event);
    }

    /// Get tools for API calls - returns Some(tools) if native function calling is enabled
    fn api_tools(&self) -> Option<Vec<crate::api::types::ToolDefinition>> {
        if self.config.agent.native_function_calling {
            Some(self.tools.definitions())
        } else {
            None
        }
    }

    /// Get current execution mode
    #[inline]
    pub fn execution_mode(&self) -> crate::config::ExecutionMode {
        self.config.execution_mode
    }

    /// Set execution mode
    #[inline]
    pub fn set_execution_mode(&mut self, mode: crate::config::ExecutionMode) {
        self.config.execution_mode = mode;
    }

    /// Cycle to next execution mode (Shift+Tab): normal → auto-edit → yolo → daemon → normal
    pub fn cycle_execution_mode(&mut self) -> crate::config::ExecutionMode {
        use crate::config::ExecutionMode;
        self.config.execution_mode = match self.config.execution_mode {
            ExecutionMode::Normal => ExecutionMode::AutoEdit,
            ExecutionMode::AutoEdit => ExecutionMode::Yolo,
            ExecutionMode::Yolo => ExecutionMode::Daemon,
            ExecutionMode::Daemon => ExecutionMode::Normal,
        };
        self.config.execution_mode
    }

    /// Check if tool execution needs confirmation based on current mode and risk level.
    ///
    /// The confirmation policy is layered:
    /// 1. Read-only tools never need confirmation
    /// 2. Yolo / Daemon mode never asks
    /// 3. Tools in `safety.require_confirmation` config always ask (except Yolo/Daemon)
    /// 4. Mode-specific rules (AutoEdit auto-approves file ops, Normal asks for everything)
    pub fn needs_confirmation(&self, tool_name: &str) -> bool {
        use crate::config::ExecutionMode;

        // Read-only tools never need confirmation
        let safe_tools = [
            "file_read",
            "directory_tree",
            "glob_find",
            "grep_search",
            "symbol_search",
            "git_status",
            "git_diff",
        ];

        if safe_tools.contains(&tool_name) {
            return false;
        }

        // Check permission store: pre-authorized grants skip confirmation
        if self.permission_store.is_authorized(tool_name, None) {
            return false;
        }

        // Yolo / Daemon never ask
        if matches!(
            self.config.execution_mode,
            ExecutionMode::Yolo | ExecutionMode::Daemon
        ) {
            return false;
        }

        // Tools in safety.require_confirmation always need confirmation
        if self
            .config
            .safety
            .require_confirmation
            .iter()
            .any(|t| t == tool_name)
        {
            return true;
        }

        match self.config.execution_mode {
            ExecutionMode::Yolo | ExecutionMode::Daemon => false, // Already handled above
            ExecutionMode::AutoEdit => {
                // Auto-approve file operations, ask for destructive operations
                !matches!(
                    tool_name,
                    "file_write" | "file_edit" | "directory_tree" | "glob_find"
                )
            }
            ExecutionMode::Normal => {
                // Ask for all tools except safe ones
                !safe_tools.contains(&tool_name)
            }
        }
    }

    /// Check if running in non-interactive mode (piped stdin)
    #[inline]
    pub fn is_interactive(&self) -> bool {
        use std::io::IsTerminal;
        std::io::stdin().is_terminal()
    }

    /// Returns true when the TUI is active and owns rendering.
    pub fn has_tui_renderer(&self) -> bool {
        crate::output::is_tui_active()
    }

    /// Shared cancellation token for Ctrl+C interrupt handling.
    pub(crate) fn cancel_token(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.cancelled)
    }

    /// Shared pause flag for the ESC listener — used by confirmation prompts.
    pub(crate) fn esc_pause_token(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.esc_paused)
    }

    /// True when the current task should stop as soon as possible.
    pub(crate) fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Relaxed)
    }

    /// Clear cancellation state after handling an interrupt.
    pub(crate) fn reset_cancellation(&self) {
        self.cancelled.store(false, Ordering::Relaxed);
    }

    /// Check if plan mode is active.
    pub fn is_plan_mode(&self) -> bool {
        self.plan_mode
    }

    /// Toggle plan mode on/off and return the new state.
    pub fn toggle_plan_mode(&mut self) -> bool {
        self.plan_mode = !self.plan_mode;
        self.plan_mode
    }

    /// Set plan mode explicitly.
    pub fn set_plan_mode(&mut self, enabled: bool) {
        self.plan_mode = enabled;
    }

    /// Get a reference to the hook registry.
    pub fn hook_registry(&self) -> &HookRegistry {
        &self.hook_registry
    }

    /// Get a mutable reference to the hook registry.
    pub fn hook_registry_mut(&mut self) -> &mut HookRegistry {
        &mut self.hook_registry
    }

    /// Resume a named chat session by loading messages from the chat store.
    /// Returns the number of messages restored on success.
    pub fn resume_named_session(&mut self, name: &str) -> Result<usize> {
        let chat = self.chat_store.load(name)?;
        self.messages = chat.messages;

        // Rebuild memory from recovered messages
        self.memory.clear();
        for msg in &self.messages {
            if msg.role != "system" {
                self.memory.add_message(msg);
            }
        }

        let count = self.messages.len();
        info!("Resumed named session '{}' with {} messages", name, count);
        Ok(count)
    }
}

#[cfg(test)]
mod tests;
