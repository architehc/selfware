//! Core agent module: LLM-driven task execution with tool orchestration.
//!
//! ## Agent lifecycle
//!
//! 1. **Initialization** (`Agent::new`): builds the API client, tool registry,
//!    safety checker, cognitive state, verification gate, and context map from
//!    a [`Config`]. Loads persisted episodic memory and self-improvement state
//!    so the agent benefits from prior sessions. Connects to any configured MCP
//!    servers and registers their tools.
//!
//! 2. **Planning** (`run_task` entry): the user's task is added as a message,
//!    a checkpoint is created, and the cognitive state is set to the Plan phase.
//!    The hierarchical context map is populated with the project tree.
//!
//! 3. **Execution loop** (`run_execution_loop`): the agent streams an LLM
//!    response, extracts tool calls, validates them through the safety checker,
//!    executes them via the `ToolRegistry`, and feeds results back. Loop control
//!    (`AgentLoop`) tracks iteration count and state transitions (Planning →
//!    Executing → Verifying → Completed/Failed).
//!
//! 4. **Verification**: after implementation, the `VerificationGate` runs
//!    project-type-specific checks (e.g. `cargo check` for Rust) to confirm
//!    the changes compile and pass tests.
//!
//! 5. **Completion / Failure**: the loop exits when the model produces a final
//!    text response with no tool calls, the iteration limit is reached, or the
//!    user cancels via Ctrl+C. Checkpoints are persisted for resumption.

use anyhow::Result;
use std::collections::{HashMap, HashSet, VecDeque};
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
use crate::cognitive::learning::ExplanationLevel;
use crate::cognitive::memory_system::MemorySystem;
use crate::cognitive::rag::RagEngine;
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
use tokio::sync::RwLock;

/// Print only when TUI is NOT active (avoids writing to stdout while
/// ratatui owns the alternate screen).
/// Uses the global output lock to prevent interleaving from concurrent tasks.
macro_rules! cli_println {
    ($($arg:tt)*) => {
        if !crate::output::is_tui_active() {
            let _lock = crate::output::OUTPUT_LOCK.lock().unwrap_or_else(|e| e.into_inner());
            println!($($arg)*);
        }
    };
}

mod assistant_response;
mod checkpointing;
pub mod compression;
pub mod context;
mod context_display;
mod context_files;
mod context_management;
pub mod context_map;
pub mod evolution_events;
mod execution;
pub mod failure_mode;
mod interactive;
pub mod last_tool;
mod learning;
pub mod loop_control;
pub mod plan_mode;
mod plan_step;
pub mod planning;
pub mod progress;
pub mod prompt_builder;
mod recovery;
mod session_log;
mod streaming;
pub mod subagent;
mod task_runner;
mod tool_collect;
mod tool_dispatch;
mod tool_validator;
pub mod tui_events;
pub mod turn_artifacts;
mod verification;
pub mod worktree;

use crate::errors::{is_confirmation_error, is_no_action_error};
use compression::CompressionOrchestrator;
use context::ContextCompressor;
use loop_control::{AgentLoop, AgentState};
use planning::Planner;
use tui_events::{AgentEvent, EventEmitter, NoopEmitter};

/// Upper bound for queued interactive messages to avoid unbounded memory growth.
pub(crate) const MAX_PENDING_MESSAGES: usize = 100;

/// Detected project type for adapting verification instructions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ProjectType {
    Python,
    JavaScript,
    TypeScript,
    Java,
    CSharp,
    Cpp,
    Sql,
    Go,
    Swift,
    Rust,
    Generic,
}

fn find_project_root_with_markers(
    start: &std::path::Path,
    markers: &[&str],
) -> Option<std::path::PathBuf> {
    start.ancestors().find_map(|ancestor| {
        markers
            .iter()
            .any(|marker| ancestor.join(marker).exists())
            .then(|| ancestor.to_path_buf())
    })
}

pub(super) fn current_project_root() -> std::path::PathBuf {
    let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    find_project_root_with_markers(
        &cwd,
        &[
            "Cargo.toml",
            "package.json",
            "tsconfig.json",
            "pyproject.toml",
            "setup.py",
            "requirements.txt",
            "go.mod",
            "pom.xml",
            "build.gradle",
            "build.gradle.kts",
            "*.csproj",
            "*.sln",
            "Package.swift",
        ],
    )
    .unwrap_or(cwd)
}

fn extract_candidate_paths_from_text(text: &str) -> Vec<String> {
    use std::sync::LazyLock;

    let mut paths = Vec::new();
    static PATH_RE: LazyLock<regex::Regex> = LazyLock::new(|| {
        regex::Regex::new(
            r#"(?:^|[\s`"'(])((?:src|tests|examples|benches)/[\w./-]+\.(?:rs|toml|md|txt|json|yaml|yml)|Cargo\.toml|README\.md|RUN_NOTES\.md)"#,
        )
        .expect("candidate path regex must compile")
    });

    for caps in PATH_RE.captures_iter(text) {
        if let Some(path) = caps.get(1) {
            let candidate = path.as_str().trim().to_string();
            if !paths.contains(&candidate) {
                paths.push(candidate);
            }
        }
    }

    paths
}
async fn read_bounded_file(path: &std::path::Path, max_chars: usize) -> Option<String> {
    let content = tokio::fs::read_to_string(path).await.ok()?;
    let bounded: String = content.chars().take(max_chars).collect();
    Some(bounded)
}

/// Detect the project type from marker files in the working directory or its ancestors.
async fn detect_project_type() -> ProjectType {
    let root = current_project_root();
    if try_exists(&root.join("pyproject.toml")).await
        || try_exists(&root.join("setup.py")).await
        || try_exists(&root.join("requirements.txt")).await
    {
        ProjectType::Python
    } else if try_exists(&root.join("tsconfig.json")).await {
        ProjectType::TypeScript
    } else if try_exists(&root.join("package.json")).await {
        ProjectType::JavaScript
    } else if try_exists(&root.join("pom.xml")).await
        || try_exists(&root.join("build.gradle")).await
        || try_exists(&root.join("build.gradle.kts")).await
    {
        ProjectType::Java
    } else if has_file_with_extension(&root, "csproj").await
        || has_file_with_extension(&root, "sln").await
    {
        ProjectType::CSharp
    } else if has_file_with_extension(&root, "cpp").await
        || has_file_with_extension(&root, "cc").await
        || has_file_with_extension(&root, "cxx").await
        || has_file_with_extension(&root, "c").await
        || try_exists(&root.join("CMakeLists.txt")).await
    {
        ProjectType::Cpp
    } else if has_file_with_extension(&root, "sql").await {
        ProjectType::Sql
    } else if try_exists(&root.join("go.mod")).await {
        ProjectType::Go
    } else if try_exists(&root.join("Package.swift")).await
        || has_file_with_extension(&root, "swift").await
    {
        ProjectType::Swift
    } else if try_exists(&root.join("Cargo.toml")).await {
        ProjectType::Rust
    } else {
        ProjectType::Generic
    }
}

async fn try_exists(path: &std::path::Path) -> bool {
    tokio::fs::try_exists(path).await.unwrap_or(false)
}

async fn has_file_with_extension(root: &std::path::Path, ext: &str) -> bool {
    match tokio::fs::read_dir(root).await {
        Ok(mut entries) => {
            while let Ok(Some(entry)) = entries.next_entry().await {
                if entry
                    .path()
                    .extension()
                    .and_then(|e| e.to_str())
                    .is_some_and(|e| e.eq_ignore_ascii_case(ext))
                {
                    return true;
                }
            }
            false
        }
        Err(_) => false,
    }
}

/// Return (verify_step, test_step, completion_rule) for the detected project type.
fn verification_instructions(pt: ProjectType) -> (&'static str, &'static str, &'static str) {
    match pt {
        ProjectType::Python => (
            "3. VERIFY: Check for syntax errors after changes. Run pytest or the project's test command if available",
            "5. TEST: Run pytest or the project's test command when implementation is complete",
            "- Verify your changes work before declaring complete",
        ),
        ProjectType::JavaScript => (
            "3. VERIFY: Check JavaScript syntax after changes and run npm/pnpm/yarn tests if available",
            "5. TEST: Run the project's JavaScript test command when implementation is complete",
            "- Verify your changes work before declaring complete",
        ),
        ProjectType::TypeScript => (
            "3. VERIFY: Run TypeScript type-checking after changes and run the project's tests if available",
            "5. TEST: Run the project's TypeScript test command when implementation is complete",
            "- Verify your changes work before declaring complete",
        ),
        ProjectType::Java => (
            "3. VERIFY: Run javac/maven/gradle checks after changes",
            "5. TEST: Run mvn test or gradle test when implementation is complete",
            "- Verify your changes work before declaring complete",
        ),
        ProjectType::CSharp => (
            "3. VERIFY: Run dotnet build after changes",
            "5. TEST: Run dotnet test when implementation is complete",
            "- Verify your changes work before declaring complete",
        ),
        ProjectType::Cpp => (
            "3. VERIFY: Run compiler or build-system checks after changes",
            "5. TEST: Run the project's C/C++ test command when implementation is complete",
            "- Verify your changes work before declaring complete",
        ),
        ProjectType::Sql => (
            "3. VERIFY: Run SQL parser/linter or database-specific validation after changes",
            "5. TEST: Run the project's database tests when implementation is complete",
            "- Verify your changes work before declaring complete",
        ),
        ProjectType::Go => (
            "3. VERIFY: Run go build after every file change",
            "5. TEST: Run go test when implementation is complete",
            "- NEVER declare complete without a successful go build",
        ),
        ProjectType::Swift => (
            "3. VERIFY: Run swift build after changes",
            "5. TEST: Run swift test when implementation is complete",
            "- Verify your changes work before declaring complete",
        ),
        ProjectType::Rust => (
            "3. VERIFY: Run cargo_check IMMEDIATELY after every file change",
            "5. TEST: Run cargo_test when implementation is complete",
            "- NEVER declare complete without a successful cargo_check",
        ),
        ProjectType::Generic => (
            "3. VERIFY: Test your changes using appropriate tools for the project type",
            "5. TEST: Verify the output works correctly when implementation is complete",
            "- Verify your changes work before declaring complete",
        ),
    }
}

/// Canonical list of tools offered when the model fails to take action.
/// Single source of truth — used by both the error recovery instructions
/// and the no-action prompt escalation in execution.rs.
pub(super) const NO_ACTION_TOOL_OPTIONS: &str =
    "directory_tree, glob_find, grep_search, file_read, shell_exec";

/// The safest fallback when the model is completely stuck: list the working directory.
pub(super) const FALLBACK_TOOL_NAME: &str = "directory_tree";
pub(super) const FALLBACK_TOOL_ARGS: &str = r#"{"path":"."}"#;

const ERROR_RECOVERY_INSTRUCTIONS: &str = r#"## WORKFLOW GUIDANCE
You are an autonomous agent with access to tools. Use tools to accomplish tasks efficiently.

### BEST PRACTICES:
1. **Start with tool calls** - When beginning work, use tools to explore or make changes
2. **Chain related edits** - When you see multiple similar bugs, fix them all in sequence without stopping to describe each one
3. **Fix then verify** - Make all your edits first, then run tests to verify
4. **Be concise** - Short explanations are fine, but focus on action

### EDIT CHAINING EXAMPLES:

GOOD: See 3 bugs → file_edit bug #1 → file_edit bug #2 → file_edit bug #3 → cargo_check
BAD: See 3 bugs → describe bug #1 → describe bug #2 → describe bug #3 → (never actually edit)

### ERROR RECOVERY (CRITICAL)
When a tool fails, you MUST try a DIFFERENT approach immediately.

Error Recovery Rules:
1. After ANY error, use a DIFFERENT tool - never retry the same tool with the same arguments
2. If file_read fails, try directory_tree, glob_find, or grep_search
3. If a command fails, try a different command or a completely different approach"#;

#[derive(Debug, Clone, PartialEq, Eq)]
struct FailedToolAttempt {
    tool_name: String,
    args_hash: u64,
    failure_kind: &'static str,
    error_preview: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
struct FileReadState {
    content_hash: u64,
    total_lines: usize,
    last_modified: Option<u64>,
    unchanged_read_count: u32,
}

/// Consolidated file-context tracking.
///
/// Groups the three previously-scattered file tracking structures into one
/// coherent unit: which files are in context, which need reload, and
/// per-file read state for redundancy detection.
struct FileTracker {
    /// Files loaded into context for reload functionality
    context_files: Vec<String>,
    /// Files modified since last loaded into context (need refresh)
    stale_files: HashSet<String>,
    /// Per-file read state used to detect redundant unchanged rereads
    read_state: HashMap<String, FileReadState>,
}

impl FileTracker {
    fn new() -> Self {
        Self {
            context_files: Vec::new(),
            stale_files: HashSet::new(),
            read_state: HashMap::new(),
        }
    }

    fn mark_stale(&mut self, path: &str) {
        if self.stale_files.len() < 500 {
            self.stale_files.insert(path.to_string());
        }
    }

    fn mark_written(&mut self, path: &str) {
        // Remove read state so next read gets a fresh baseline
        self.read_state.remove(path);
        self.mark_stale(path);
    }

    fn remove_deleted(&mut self, path: &str) {
        self.read_state.remove(path);
        self.stale_files.remove(path);
        self.context_files.retain(|p| p != path);
    }
}

const TASK_STATE_NOTE_LIMIT: usize = 16;

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
    /// Enforces the YOLO-mode safety floor (forbidden operations, protected
    /// paths, destructive shell/git-push gating, dangerous container mounts)
    /// that must hold even when `needs_confirmation()` says no prompt is
    /// needed. See `confirm_tool_execution` in `tool_dispatch.rs`.
    yolo_manager: crate::safety::yolo::YoloManager,
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
    /// Tools the task explicitly requires before the agent may answer.
    required_task_tools: std::collections::BTreeSet<String>,
    /// Verification gate for automatic code validation
    verification_gate: VerificationGate,
    /// Error analyzer for intelligent error suggestions
    error_analyzer: ErrorAnalyzer,
    /// Consolidated file tracking: context files, stale files, and read state.
    file_tracker: FileTracker,
    /// Recent task-state notes surfaced in debug output.
    task_state_notes: VecDeque<String>,
    /// Last time a checkpoint was persisted to disk
    last_checkpoint_persisted_at: Instant,
    /// Tool call count at last persisted checkpoint
    last_checkpoint_tool_calls: usize,
    /// Whether at least one checkpoint has been persisted in this session
    checkpoint_persisted_once: bool,
    /// Event emitter for real-time updates (TUI or other)
    events: Arc<dyn EventEmitter>,
    /// Receives the user's y/n answer to a TUI permission prompt. `None`
    /// when not running under the TUI (or the TUI never wired one up) --
    /// in that case a confirmation-gated tool must fall back to the CLI
    /// prompt rather than being silently approved or denied.
    ///
    /// Wrapped in `Arc<Mutex<..>>` (rather than held directly) so `Agent`
    /// stays `Sync` -- some call sites `tokio::spawn` futures that hold
    /// `&Agent` across an await point.
    permission_response_rx: Option<Arc<std::sync::Mutex<std::sync::mpsc::Receiver<bool>>>>,
    /// Structured progress emitter (stderr / TUI / future Prometheus, etc).
    /// Defaults to a no-op; set to a `StderrProgressEmitter` for headless runs.
    progress_emitter: Arc<dyn progress::ProgressEmitter>,
    /// Edit history for undo support
    edit_history: EditHistory,
    /// Last assistant response content (for /copy command)
    last_assistant_response: String,
    /// Chat session store for save/resume/list/delete
    chat_store: ChatStore,
    /// Cancellation token set by Ctrl+C while a task is running
    cancelled: Arc<AtomicBool>,
    /// Messages queued for sequential execution
    pending_messages: VecDeque<PendingMessage>,
    /// Maximum total estimated tokens for the message history.
    /// When exceeded, oldest non-system messages are removed.
    max_context_tokens: usize,
    /// Self-healing engine for automatic recovery attempts
    #[cfg(feature = "resilience")]
    self_healing: SelfHealingEngine,
    /// Recent tool call signatures for repetition detection (name, args_hash)
    recent_tool_calls: VecDeque<(String, u64)>,
    /// Recent per-step tool batches for oscillation detection.
    recent_tool_batches: VecDeque<Vec<(String, u64)>>,
    /// Failed tool attempts in the current recovery window.
    recent_failed_tool_attempts: VecDeque<FailedToolAttempt>,
    /// args-hashes of file_edit calls already escalated to file_write. Prevents
    /// re-reading and re-injecting the whole target file on every repeat of the
    /// same failing edit (EDIT-RETRY-REINJECT context bloat).
    escalated_edit_args_hashes: std::collections::HashSet<u64>,
    /// Hook registry for event-driven automation
    hook_registry: HookRegistry,
    /// Plan mode: propose tool calls without executing them
    plan_mode: bool,
    /// Plan mode manager for structured plan mode with approval workflow
    pub plan_mode_manager: plan_mode::PlanModeManager,
    /// Audit logger for JSONL tool execution logging
    audit_logger: Option<crate::safety::audit::AuditLogger>,
    /// Persistent per-session execution log with raw args/results
    session_logger: Option<session_log::SessionLogger>,
    /// One-shot reminder injected after a failed tool call.
    pending_failure_hint: Option<String>,
    /// When set, contains the task description for a phase-2 synthesis call.
    /// The next execution step will make a tool-free API call to produce a
    /// text answer from data already in context.
    pending_synthesis: Option<String>,
    /// Consecutive turns where the model described intent but emitted no tool call.
    consecutive_no_action_prompts: usize,
    /// Consecutive no-tool-call turns on a read-only (non-mutating) task. Reset by
    /// any real tool call. Used to force-finalize a read-only task that keeps
    /// narrating without answering, instead of spinning to MAX_ITERATIONS.
    readonly_no_tool_streak: usize,
    /// Lifetime count of completion-gate rejections on a mutation-required task
    /// with zero mutating calls. NOT reset by read-only tool calls or recovery
    /// nudges (that reset is what previously let this loop burn 100 iterations);
    /// reset only at task start and on a real mutating tool call.
    mutation_gate_rejections: usize,
    /// Consecutive StaleVerification rejections (edit done, not yet verified) where
    /// the model produced a tool-less answer instead of running the test. Used to
    /// auto-run verification and break the churn.
    consecutive_stale_verification: usize,
    /// Lifetime total of no-action prompts across the entire task.
    /// Unlike the consecutive counter, this is NOT reset when the model produces
    /// a non-intent response. It provides a hard abort ceiling.
    total_no_action_prompts: usize,
    /// Hash of the most recent no-action assistant content used for loop detection.
    last_no_action_prompt_hash: Option<u64>,
    /// Permission store for pre-authorized tool grants
    permission_store: crate::safety::permissions::PermissionStore,
    /// Unified cache manager for tool results and LLM responses (long-term memory)
    cache_manager: crate::session::cache::CacheManager,
    /// Concurrency governor for limiting concurrent streams and tool executions
    governor: ConcurrencyGovernor,
    /// Pause flag for the ESC listener — set when a confirmation prompt needs stdin
    esc_paused: Arc<AtomicBool>,
    /// Acknowledgement from the ESC listener that it observed the pause flag.
    esc_pause_ack: Arc<AtomicBool>,
    /// Last tool output for progressive disclosure via `/last`.
    last_tool_output: Option<last_tool::LastToolOutput>,
    /// Monotonic counter for `<workdir>/.selfware/turns/turn_NNNN.json` files.
    /// Incremented once per LLM call that produces a captured artifact.
    turn_artifact_seq: usize,
    /// Recent screenshot hashes for visual stuck-loop detection.
    recent_screenshot_hashes: std::collections::VecDeque<u64>,
    /// Whether a visual stuck loop was detected on the most recent screenshot.
    visual_stuck_loop_active: bool,
    /// Advanced visual state tracker for stuck-loop detection with perceptual hashing
    visual_state_tracker: crate::testing::visual_verification::VisualStateTracker,
    /// Hierarchical context map for token-aware codebase ingestion.
    context_map: context_map::ContextMap,
    /// RAG engine for semantic code search via `/scan`
    rag_engine: Option<Arc<RwLock<RagEngine>>>,
    /// Detail level for `/explain` code education output.
    explanation_level: ExplanationLevel,
    /// Consecutive tool calls that were suppressed (retry suppressed / no-op).
    /// When this exceeds a threshold the agent forces completion instead of
    /// looping until max_iterations.
    consecutive_suppressions: usize,
    /// Tracks consecutive steps that only used read-only or verification tools
    /// (file_read, directory_tree, grep_search, shell_exec cargo test/check, etc.)
    /// without any state-changing tool. When this exceeds a threshold, recovery
    /// logic nudges or blocks more read-only batches until the agent edits code
    /// or otherwise changes project state.
    consecutive_read_only_steps: usize,
    /// Number of times a post-mutation observational shell batch has been
    /// detected as a repetition loop. Small models often need 1–2 verification
    /// cycles to repair syntax errors, so the agent is only hard-stopped after
    /// this counter exceeds a small threshold.
    post_edit_observational_shell_count: usize,
    /// How many times the terminal progress guard fired without producing a write.
    /// After 2 hits the run fails with READ_LOOP_NO_EDIT.
    terminal_guard_hits: usize,
    /// Most recently read file path (set by file_read).  Used to inject
    /// concrete edit templates when the model is stuck in a read loop.
    last_read_file: Option<String>,
    /// Set to true once any file_write or file_edit has been successfully executed
    /// (including synthetic/auto-writes). Used by progress guard to relax thresholds.
    has_written_any_file: bool,
    /// Set to true once the assistant has emitted a FILES: checklist naming the
    /// files it intends to change. Prevents premature edits before the model has
    /// identified the relevant source files.
    files_checklist_seen: bool,
    /// Set to true when the progress guard has just issued a force-mutation
    /// directive. The very next mutating edit is allowed to bypass the FILES:
    /// checklist guard so the model can recover from a read-only loop.
    force_mutation_pending: bool,
    /// Monotonic sequence incremented after every successful state-changing tool.
    mutation_sequence: usize,
    /// Mutation sequence number covered by the most recent successful verification.
    last_successful_verification_mutation_sequence: usize,
    /// Most recent failed verification summary, used by the completion gate.
    last_failed_verification_summary: Option<String>,
    /// Three-layer context compression orchestrator
    compression_orchestrator: CompressionOrchestrator,
    /// Lifetime count of successful mutating tool calls (file_write/file_edit/file_delete/etc.)
    /// for this task. Drives `FailureMode::classify`.
    mutating_tool_call_count: usize,
    /// Lifetime count of all attempted tool calls (success + failure). Drives
    /// `FailureMode::classify` to compute read/total ratios.
    total_tool_call_count: usize,
    /// Number of times the read-only progress guard fired during this task.
    progress_guard_fire_count: usize,
    /// Tool calls that have been hard-blocked (e.g. by `STUCK_LOOP_HARD_BLOCK_AFTER`)
    /// after repeated failures. Length feeds the RetryLoop classification.
    permanently_blocked_tool_calls: Vec<String>,
    /// Count of HTTP 400 "Assistant response prefill incompatible" responses
    /// observed from the API. 3+ trips the prefill circuit breaker.
    prefill_400_count: usize,
    /// Whether the prefill-incompatible circuit breaker is currently open.
    prefill_breaker_open: bool,
    /// Cumulative token usage across all LLM calls in the current task.
    cumulative_token_usage: crate::observability::dashboard::TokenUsage,
    /// Failure mode from the most recent run (set by `finalize_failure_mode`).
    last_run_failure_mode: Option<failure_mode::FailureMode>,
    /// Wall-clock start time of the current task.
    task_start_time: std::time::Instant,
}

impl Agent {
    /// Construct a fully-initialised agent from the given configuration.
    ///
    /// Sets up the API client, tool registry (with built-in + MCP tools),
    /// safety checker, cognitive state (with persisted episodic memory),
    /// verification gate tuned to the detected project type, and the
    /// context compression pipeline. Returns an error if the API client
    /// cannot be created (e.g. missing API key).
    pub async fn new(config: Config) -> Result<Self> {
        let cache_config = config.cache.clone();
        let client = ApiClient::new(&config)?;
        let mut tools = ToolRegistry::with_safety_config(Some(&config.safety));
        tools.register_critical(crate::tools::fim::FileFimEdit::with_safety_config(
            std::sync::Arc::new(client.clone()),
            config.safety.clone(),
        ));
        tools.rebuild_search_index();
        let memory = AgentMemory::new(&config)?;
        let safety = SafetyChecker::new(&config.safety);
        // Keep global init as fallback for tools not yet migrated to per-instance config.
        init_safety_config(&config.safety);
        let yolo_manager = {
            use crate::safety::yolo::YoloConfig as SafetyYoloConfig;
            let is_yolo_mode = matches!(
                config.execution_mode,
                crate::config::ExecutionMode::Yolo | crate::config::ExecutionMode::Daemon
            );
            crate::safety::yolo::YoloManager::new(SafetyYoloConfig {
                enabled: is_yolo_mode || config.yolo.enabled,
                max_operations: config.yolo.max_operations,
                max_hours: config.yolo.max_hours,
                allow_git_push: config.yolo.allow_git_push,
                allow_destructive_shell: config.yolo.allow_destructive_shell,
                audit_log_path: config.yolo.audit_log_path.clone(),
                status_interval: config.yolo.status_interval,
                ..Default::default()
            })
        };
        let loop_control = AgentLoop::new(config.agent.max_iterations);
        // Compressor is created later, after max_context_tokens is calculated.
        // See the block near "Calculate max_context_tokens" below.
        let compressor_content_ratio = config.agent.context_content_ratio;

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

        let self_improvement = if tokio::fs::try_exists(&improvement_engine_path)
            .await
            .unwrap_or(false)
        {
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
        let project_type = detect_project_type().await;
        let (verify_step, test_step, completion_rule) = verification_instructions(project_type);
        info!("Detected project type: {:?}", project_type);

        // Build system prompt using Static/Dynamic boundary system
        // This provides 60-80% token reduction and 70%+ cache hit rates
        let mut prompt_builder = prompt_builder::SystemPromptBuilder::new();

        // Tool discovery message - explains deferred tool loading
        let tool_discovery_note = r#"## TOOL DISCOVERY
You have access to a focused set of critical tools. Additional specialized tools (git, cargo, containers, browser, etc.) can be discovered using the `tool_search` tool.

To find more tools: <tool><name>tool_search</name><arguments>{"query": "git"}</arguments></tool>

Found tools become available immediately for the rest of the session."#;

        // === STATIC SECTIONS (cached across conversations) ===
        // Core identity and workflow - doesn't change between sessions
        if config.agent.native_function_calling {
            info!("Using native function calling mode");
            prompt_builder.add_static(format!(
                r#"You are Selfware, an expert software engineering AI assistant.

You have access to critical tools for file operations, search, and shell commands.
Additional tools can be discovered using tool_search.

{}

## MANDATORY WORKFLOW
1. PLAN: Understand what needs to change — read relevant files first
2. IMPLEMENT: Make code changes using file_edit or file_write
{}
4. FIX: If verification fails, fix errors before proceeding
{}

## EFFICIENCY RULES
- To read multiple files at once, use context_bulk_read with a glob pattern (e.g. "src/agent/*.rs")
- For read-only tasks (summarize, explain, review), you do NOT need cargo_check — just provide your answer
- Use grep_search to find specific code instead of reading entire files
- Use directory_tree to understand structure before reading files
- Need git, cargo, containers, or other tools? Use tool_search to discover them

## CRITICAL RULES
- **IMMEDIATE TOOL EXECUTION**: Your FIRST response must be a tool call. NEVER output text like "I'll..." or "Let me..." before calling tools.
- NEVER skip verification after file_edit or file_write
{}
- When editing files, include 3-5 lines of context for unique matches
- You have a large budget. Do NOT rush. Be thorough and methodical.
- When the task is complete, respond with a summary of what was done."#,
                tool_discovery_note, verify_step, test_step, completion_rule
            ));
            prompt_builder.add_static(ERROR_RECOVERY_INSTRUCTIONS.to_string());
        } else {
            // XML-based: embed CRITICAL tools only in system prompt
            // Deferred tools are discovered via tool_search, reducing context window usage
            let critical_tools = tools.list_critical();
            let deferred_count = tools.total_count() - critical_tools.len();

            info!(
                "Using deferred tool loading: {} critical tools, {} deferred",
                critical_tools.len(),
                deferred_count
            );

            let mut tool_desc_parts: Vec<String> = critical_tools
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
                .collect();

            // Add context management tools.
            for ctx_tool in crate::tools::context::context_tool_descriptions() {
                tool_desc_parts.push(format!(
                    r#"<tool name="{}">
  <description>{}</description>
  <parameters>{}</parameters>
</tool>"#,
                    ctx_tool.name, ctx_tool.description, ctx_tool.schema
                ));
            }
            let tool_descriptions = tool_desc_parts.join("\n");

            prompt_builder.add_static(format!(
                r#"You are Selfware, an expert software engineering AI assistant with access to tools.

Available tools ({} shown, {} more available via tool_search):
{}

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
<arguments>{{"path": "src/main.rs"}}</arguments>
</tool>

<tool>
<name>file_edit</name>
<arguments>{{"path": "src/main.rs", "old_str": "fn main() {{", "new_str": "fn main() {{\n    println!(\"hello\");"}}</arguments>
</tool>

<tool>
<name>shell_exec</name>
<arguments>{{"command": "cargo check"}}</arguments>
</tool>

### WRONG formats (DO NOT USE):
- file_read("path") — WRONG, must use XML tags
- <function>tool_name</function> — WRONG
- Any format other than <tool><name>...</name><arguments>...</arguments></tool> — WRONG

## MANDATORY WORKFLOW
1. PLAN: Understand what needs to change — read relevant files first
2. IMPLEMENT: Make code changes using file_edit or file_write
{}
4. FIX: If verification fails, fix errors before proceeding
{}

## EFFICIENCY RULES
- To read multiple files at once, use context_bulk_read with a glob pattern (e.g. "src/agent/*.rs")
- For read-only tasks (summarize, explain, review), you do NOT need cargo_check — just provide your answer
- Use grep_search to find specific code instead of reading entire files
- Use directory_tree to understand structure before reading files
- Need git, cargo, containers, or other tools? Use tool_search to discover them

## CRITICAL RULES
- **IMMEDIATE TOOL EXECUTION**: Your FIRST response must be a tool call. NEVER output text like "I'll..." or "Let me..." before calling tools.
- Use <name>TOOL_NAME</name> - never <function>
- Arguments must be valid JSON inside <arguments>...</arguments>
- Each <tool>...</tool> block is executed separately
- Wait for tool results before proceeding
- NEVER skip verification after file_edit or file_write
{}
- You have a large budget. Do NOT rush. Be thorough and methodical.
- When done, respond with plain text only (no tool tags)"#,
                critical_tools.len(), deferred_count, tool_descriptions,
                tool_discovery_note, verify_step, test_step, completion_rule
            ));
            prompt_builder.add_static(ERROR_RECOVERY_INSTRUCTIONS.to_string());
        }

        // === DYNAMIC SECTIONS (computed fresh per request) ===
        // Memory files - changes based on working directory
        let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
        let memory_files = MemorySystem::discover(&cwd);
        let memory_section = if !memory_files.is_empty() {
            let section = MemorySystem::format_for_prompt(&memory_files);
            info!(
                "Injected {} memory file(s) into system prompt",
                memory_files.len()
            );
            section
        } else {
            String::new()
        };
        prompt_builder.add_dynamic(move || memory_section.clone());

        let workspace_guidance_files = MemorySystem::discover_workspace_guidance(&cwd);
        let workspace_guidance_section = if !workspace_guidance_files.is_empty() {
            let section =
                MemorySystem::format_workspace_guidance_for_prompt(&workspace_guidance_files);
            info!(
                "Injected {} workspace guidance file(s) into system prompt",
                workspace_guidance_files.len()
            );
            section
        } else {
            String::new()
        };
        prompt_builder.add_dynamic(move || workspace_guidance_section.clone());

        // Episodic lessons - changes as agent learns
        let lessons: Vec<String> = cognitive_state.episodic_memory.recent_lessons(10);
        prompt_builder.add_dynamic(move || {
            if lessons.is_empty() {
                String::new()
            } else {
                let mut section = String::from("\n\n## Global Lessons Learned\nDo not repeat past mistakes. Consider these lessons:\n");
                for lesson in &lessons {
                    section.push_str(&format!("- {}\n", lesson));
                }
                section
            }
        });

        // Build the prompt with cache key
        let (static_cache_key, system_prompt) = prompt_builder.build_cached();
        info!(
            "System prompt built with static cache key: {} ({} static sections, {} dynamic sections)",
            static_cache_key,
            prompt_builder.static_section_count(),
            prompt_builder.dynamic_section_count()
        );

        // Apply evolved prompt if available (this modifies the full prompt)
        let mut final_prompt = system_prompt;
        if let Some(tournament) = self_improvement.evolve_prompt(&final_prompt, "system_prompt") {
            if tournament.winner_prompt != final_prompt {
                info!(
                    "Applied evolved system prompt variant '{}' (predicted quality {:.2})",
                    tournament.winner_strategy, tournament.winner_score
                );
                final_prompt = tournament.winner_prompt;
            }
        }

        let messages = vec![Message::system(final_prompt)];

        // Initialize checkpoint manager if configured
        let checkpoint_manager = CheckpointManager::default_path().ok();

        // Initialize verification gate with project root
        let project_root = current_project_root();
        let mut verification_gate = VerificationGate::new(&project_root, VerificationConfig::fast());
        if let Some(ref cmd) = config.agent.post_edit_test_command {
            verification_gate.set_post_edit_test_command(Some(cmd.clone()));
        }

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
                                // Register MCP tools as CRITICAL so they appear in the
                                // initial `tools: [...]` array sent to the LLM in native
                                // function-calling mode, and pass the activation gate in
                                // XML mode. When a user explicitly configures an MCP
                                // server via `[[mcp.servers]]`, they want those tools
                                // visible immediately — not hidden behind tool_search.
                                for tool in mcp_tools {
                                    tools.register_critical(tool);
                                }
                                info!(
                                    "Registered {} tool(s) from MCP server '{}' as critical",
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
        let session_logger = session_log::SessionLogger::new(&session_id).await;
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

        // Calculate max_context_tokens before moving config.
        // context_length MUST match vLLM --max-model-len exactly.
        // Reserve: output tokens (max_tokens) + 20% safety margin for tool
        // definitions, chat template formatting, and token estimation variance.
        // The old fixed 200K reserve was far too aggressive — it left only 46K
        // usable tokens on a 262K context model, causing premature eviction of
        // file content and tool-call retry loops.
        let model_context_limit = config.context_length;
        let safety_margin = model_context_limit / 5; // 20% of context window
        let max_context_tokens = model_context_limit
            .saturating_sub(config.max_tokens) // reserve for output tokens
            .saturating_sub(safety_margin); // tools + template + estimation safety

        if max_context_tokens < 2048 {
            anyhow::bail!(
                "max_context_tokens too small ({}). context_length={}, max_tokens={}. \
                 Increase context_length or decrease max_tokens so at least 2048 tokens remain for conversation.",
                max_context_tokens, model_context_limit, config.max_tokens
            );
        }
        tracing::info!(
            "Context limits: model={}, max_context_tokens={} (safety_margin={}), token_budget={}",
            model_context_limit,
            max_context_tokens,
            safety_margin,
            config.agent.token_budget
        );

        // Use max_context_tokens (the actual usable conversation budget) rather
        // than token_budget (which defaults to max_tokens = output budget).
        let ctx_map = context_map::ContextMap::new(
            max_context_tokens,
            config.agent.context_content_ratio,
            config.agent.context_compression_ratio,
            config.agent.context_thinking_ratio,
        );
        // Create compressor with the full conversation budget, not output budget.
        // The old value (max_tokens=16384) triggered compression at ~12K tokens,
        // evicting file content after just a few tool calls.
        let compressor =
            ContextCompressor::with_content_ratio(max_context_tokens, compressor_content_ratio);
        let governor = ConcurrencyGovernor::from_config(&config.concurrency);

        let agent = Self {
            client,
            tools,
            memory,
            safety,
            yolo_manager,
            config,
            loop_control,
            messages,
            compressor,
            checkpoint_manager,
            current_checkpoint: None,
            cognitive_state,
            self_improvement,
            current_task_context: String::new(),
            required_task_tools: std::collections::BTreeSet::new(),
            verification_gate,
            error_analyzer,
            file_tracker: FileTracker::new(),
            task_state_notes: VecDeque::new(),
            last_checkpoint_persisted_at: Instant::now(),
            last_checkpoint_tool_calls: 0,
            checkpoint_persisted_once: false,
            events: Arc::new(NoopEmitter),
            permission_response_rx: None,
            progress_emitter: Arc::new(progress::NoopProgressEmitter),
            edit_history,
            last_assistant_response: String::new(),
            chat_store,
            cancelled: Arc::new(AtomicBool::new(false)),
            pending_messages: VecDeque::new(),
            // max_context_tokens calculated above to stay within token_budget
            // after accounting for safety_margin and tool definition tokens
            max_context_tokens,
            #[cfg(feature = "resilience")]
            self_healing,
            recent_tool_calls: VecDeque::new(),
            recent_tool_batches: VecDeque::new(),
            recent_failed_tool_attempts: VecDeque::new(),
            escalated_edit_args_hashes: std::collections::HashSet::new(),
            hook_registry,
            plan_mode,
            plan_mode_manager: plan_mode::PlanModeManager::new(),
            audit_logger,
            session_logger,
            pending_failure_hint: None,
            pending_synthesis: None,
            consecutive_no_action_prompts: 0,
            readonly_no_tool_streak: 0,
            mutation_gate_rejections: 0,
            consecutive_stale_verification: 0,
            total_no_action_prompts: 0,
            last_no_action_prompt_hash: None,
            permission_store,
            cache_manager: crate::session::cache::CacheManager::new(cache_config),
            governor,
            esc_paused: Arc::new(AtomicBool::new(false)),
            esc_pause_ack: Arc::new(AtomicBool::new(false)),
            last_tool_output: None,
            turn_artifact_seq: 0,
            recent_screenshot_hashes: std::collections::VecDeque::new(),
            visual_stuck_loop_active: false,
            visual_state_tracker:
                crate::testing::visual_verification::VisualStateTracker::default_config(),
            context_map: ctx_map,
            rag_engine: None,
            explanation_level: ExplanationLevel::Intermediate,
            consecutive_suppressions: 0,
            consecutive_read_only_steps: 0,
            post_edit_observational_shell_count: 0,
            terminal_guard_hits: 0,
            last_read_file: None,
            has_written_any_file: false,
            files_checklist_seen: false,
            force_mutation_pending: false,
            mutation_sequence: 0,
            last_successful_verification_mutation_sequence: 0,
            last_failed_verification_summary: None,
            compression_orchestrator: CompressionOrchestrator::new(),
            mutating_tool_call_count: 0,
            total_tool_call_count: 0,
            progress_guard_fire_count: 0,
            permanently_blocked_tool_calls: Vec::new(),
            prefill_400_count: 0,
            prefill_breaker_open: false,
            cumulative_token_usage: crate::observability::dashboard::TokenUsage::default(),
            last_run_failure_mode: None,
            task_start_time: Instant::now(),
        };

        let reconcile_report = crate::tools::process::reconcile_managed_processes(true).await;
        let inventory = crate::tools::process::process_inventory(5).await;
        agent.log_session_start_event();
        agent.log_process_reconcile_event("session_start", reconcile_report);
        agent.log_process_inventory_event("session_start", inventory);
        Ok(agent)
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

    /// Wire up the channel the TUI uses to answer permission prompts
    /// (see `AgentEvent::PermissionRequested` / `await_tui_permission_response`).
    #[cfg(feature = "tui")]
    pub fn with_permission_channel(mut self, rx: std::sync::mpsc::Receiver<bool>) -> Self {
        self.permission_response_rx = Some(Arc::new(std::sync::Mutex::new(rx)));
        self
    }

    /// Block (off the async executor) until the TUI answers a permission
    /// prompt this agent just emitted via `AgentEvent::PermissionRequested`.
    ///
    /// Fails closed: if the TUI never wired up a response channel, or the
    /// channel is disconnected (TUI thread exited), the tool is denied
    /// rather than silently approved.
    async fn await_tui_permission_response(&mut self) -> bool {
        let Some(rx) = self.permission_response_rx.clone() else {
            warn!("TUI permission prompt requested but no response channel is wired up; denying");
            return false;
        };
        tokio::task::spawn_blocking(move || {
            rx.lock()
                .map(|guard| guard.recv().unwrap_or(false))
                .unwrap_or(false)
        })
        .await
        .unwrap_or(false)
    }

    /// Attach an evolution event bus. All existing AgentEvents will be
    /// automatically translated to EvolutionEvents via the bridge emitter.
    /// The inner emitter (TUI or noop) is preserved — events flow to both.
    pub fn with_evolution_bus(
        mut self,
        bus: evolution_events::EvolutionBus,
        agent_id: String,
    ) -> Self {
        self.events = Arc::new(evolution_events::EvolutionBridgeEmitter::new(
            bus,
            agent_id,
            Arc::clone(&self.events),
        ));
        self
    }

    /// Emit an event to the TUI / event listener (no-op when no emitter is configured).
    fn emit_event(&self, event: AgentEvent) {
        self.events.emit(event);
    }

    /// Swap in a custom [`progress::ProgressEmitter`].  Used by the headless
    /// non-TUI path to attach a [`progress::StderrProgressEmitter`].
    ///
    /// The emitter is also propagated into the inner [`ApiClient`] so HTTP
    /// round-trip events (`LlmRequestSent` / `LlmResponseReceived`) land on
    /// the same channel as step / tool / guard events.
    pub fn with_progress_emitter(mut self, emitter: Arc<dyn progress::ProgressEmitter>) -> Self {
        self.progress_emitter = Arc::clone(&emitter);
        self.client.with_progress_emitter(emitter);
        self
    }

    /// Emit a structured progress event (no-op when the emitter is the default).
    pub(super) fn emit_progress(&self, event: progress::ProgressEvent) {
        self.progress_emitter.emit(event);
    }

    /// Read-only access to the configured progress emitter — used by helpers
    /// that need to clone the `Arc` (e.g. the tool dispatcher).
    #[allow(dead_code)]
    pub(super) fn progress_emitter(&self) -> Arc<dyn progress::ProgressEmitter> {
        Arc::clone(&self.progress_emitter)
    }

    fn collect_synthesis_tool_history(&self) -> String {
        let mut tool_history = String::new();

        for msg in self.messages.iter().rev().take(24).rev() {
            let text = msg.content.text_all();
            let looks_like_tool_result = msg.role == "tool" || text.contains("<tool_result>");
            if !looks_like_tool_result {
                continue;
            }

            let cleaned = text
                .replace("<tool_result>", "")
                .replace("</tool_result>", "")
                .replace("<error>", "")
                .replace("</error>", "");
            let cleaned = cleaned.trim();
            if cleaned.len() < 40 {
                continue;
            }

            let bounded: String = cleaned.chars().take(6000).collect();
            tool_history.push_str(&format!("\n--- tool result ---\n{}\n", bounded));
            if tool_history.len() >= 24_000 {
                break;
            }
        }

        tool_history
    }

    async fn collect_direct_project_context(&self, task: &str) -> String {
        const MAX_FILES: usize = 8;
        const MAX_CHARS_PER_FILE: usize = 8_000;
        const MAX_TOTAL_CHARS: usize = 40_000;

        async fn add_candidate_path(
            root: &std::path::Path,
            relative: String,
            candidates: &mut Vec<String>,
            seen: &mut HashSet<String>,
        ) {
            if candidates.len() >= MAX_FILES
                || relative.is_empty()
                || !seen.insert(relative.clone())
            {
                return;
            }

            let full = root.join(&relative);
            let is_file = tokio::fs::metadata(&full)
                .await
                .map(|m| m.is_file())
                .unwrap_or(false);
            if is_file {
                candidates.push(relative);
            }
        }

        let root = current_project_root();
        let mut candidates = Vec::new();
        let mut seen = HashSet::new();

        for relative in extract_candidate_paths_from_text(task) {
            add_candidate_path(&root, relative, &mut candidates, &mut seen).await;
        }

        for relative in extract_candidate_paths_from_text(self.learning_context()) {
            add_candidate_path(&root, relative, &mut candidates, &mut seen).await;
        }

        for msg in self.messages.iter().rev().take(20) {
            for relative in extract_candidate_paths_from_text(&msg.content.text_all()) {
                add_candidate_path(&root, relative, &mut candidates, &mut seen).await;
            }
        }

        for relative in [
            "Cargo.toml",
            "src/lib.rs",
            "src/main.rs",
            "README.md",
            "RUN_NOTES.md",
        ] {
            add_candidate_path(&root, relative.to_string(), &mut candidates, &mut seen).await;
        }

        for folder in ["src", "tests"] {
            let folder_path = root.join(folder);
            if !tokio::fs::try_exists(&folder_path).await.unwrap_or(false) {
                continue;
            }

            let mut discovered = Vec::new();
            let root_for_walk = root.clone();
            let folder_path = folder_path.clone();
            let walk_result = tokio::task::spawn_blocking(move || {
                let mut out = Vec::new();
                for entry in walkdir::WalkDir::new(&folder_path)
                    .max_depth(3)
                    .into_iter()
                    .filter_map(Result::ok)
                {
                    let path = entry.path();
                    if !entry.file_type().is_file() {
                        continue;
                    }
                    let Some(ext) = path.extension().and_then(|ext| ext.to_str()) else {
                        continue;
                    };
                    if !matches!(ext, "rs" | "toml" | "md") {
                        continue;
                    }
                    let Some(relative) = path.strip_prefix(&root_for_walk).ok().and_then(|p| p.to_str()) else {
                        continue;
                    };
                    out.push(relative.replace('\\', "/"));
                }
                out
            })
            .await
            .unwrap_or_default();
            discovered.extend(walk_result);
            discovered.sort();

            for relative in discovered {
                add_candidate_path(&root, relative, &mut candidates, &mut seen).await;
                if candidates.len() >= MAX_FILES {
                    break;
                }
            }

            if candidates.len() >= MAX_FILES {
                break;
            }
        }

        let mut file_context = String::new();
        for relative in candidates {
            if file_context.len() >= MAX_TOTAL_CHARS {
                break;
            }

            let full = root.join(&relative);
            let Some(content) = read_bounded_file(&full, MAX_CHARS_PER_FILE).await else {
                continue;
            };
            file_context.push_str(&format!("\n--- {} ---\n{}\n", relative, content));
        }

        file_context
    }

    /// Phase-2 synthesis: make a single tool-free API call to produce a text
    /// answer from data already in context. Used when the model gathered data
    /// via tools but can't transition to a text response (Qwen3.5 limitation).
    ///
    /// Builds a minimal prompt with just the task + gathered data, no tool
    /// definitions, no XML. Forces the model to produce text.
    pub(super) async fn synthesize_answer(&mut self, task: &str) -> Result<Option<String>> {
        let mut file_context = String::new();

        // Collect file contents from context map (the data gathered in phase 1).
        // Try Full level first, then fall back to Skeleton (signatures).
        for path in self
            .context_map
            .files_at_level(context_map::ContextLevel::Full)
        {
            if let Some(content) = self.context_map.full_content(path) {
                file_context.push_str(&format!("\n--- {} ---\n{}\n", path.display(), content));
            }
        }

        // Fall back to Skeleton level if no Full content available.
        if file_context.is_empty() {
            for path in self
                .context_map
                .files_at_level(context_map::ContextLevel::Skeleton)
            {
                if let Some(skeleton) = self.context_map.skeleton(path) {
                    file_context.push_str(&format!(
                        "\n--- {} (signatures) ---\n{}\n",
                        path.display(),
                        skeleton.render()
                    ));
                }
            }
        }

        let is_mutation_task = tool_dispatch::task_requires_mutation(task);
        if file_context.is_empty() && is_mutation_task {
            file_context = self.collect_direct_project_context(task).await;
        }

        let tool_history = self.collect_synthesis_tool_history();

        if is_mutation_task && file_context.is_empty() {
            return Ok(None);
        }

        if file_context.is_empty() && tool_history.is_empty() {
            return Ok(None);
        }

        let context_data = if tool_history.is_empty() {
            file_context.clone()
        } else if file_context.is_empty() {
            format!("\n--- tool history ---\n{}\n", tool_history)
        } else {
            format!(
                "{}\n--- recent tool history ---\n{}\n",
                file_context, tool_history
            )
        };

        let synthesis_prompt = if tool_dispatch::task_requires_mutation(task) {
            format!(
                "You are helping with a code-change task. Use ONLY the provided file contents.\n\n\
                 TASK: {}\n\n\
                 FILE CONTENTS:\n{}\n\n\
                 If you can fix the task, output the exact replacement code needed.\n\
                 Include the target file path in plain text and then a fenced code block with the full replacement content.\n\
                 Do NOT describe what you would do. Produce the code directly.\n\
                 If tests or notes also need updates and you have enough context, include those replacements too.\n\
                 If the provided context is insufficient, say exactly which file is missing.",
                task, context_data
            )
        } else {
            format!(
                "You are a helpful assistant. Answer the following task based ONLY on the provided file contents.\n\n\
                 TASK: {}\n\n\
                 FILE CONTENTS:\n{}\n\n\
                 Provide your answer now. Be concise and direct.",
                task, context_data
            )
        };

        let messages = vec![
            crate::api::types::Message::system(synthesis_prompt),
            crate::api::types::Message::user(task.to_string()),
        ];

        // No tools, no streaming — just a direct completion
        let response = self
            .client
            .chat(messages, None, crate::api::ThinkingMode::Disabled)
            .await?;

        let answer = response
            .choices
            .first()
            .map(|c| c.message.content.text().to_string())
            .unwrap_or_default();

        if answer.trim().is_empty() {
            Ok(None)
        } else {
            Ok(Some(answer))
        }
    }

    /// Get tools for API calls - returns Some(tools) if native function calling is enabled
    fn api_tools(&self) -> Option<Vec<crate::api::types::ToolDefinition>> {
        if self.config.agent.native_function_calling {
            Some(self.tools.definitions())
        } else {
            None
        }
    }

    /// Build LSP-enriched context for a file, helping smaller models understand code semantics.
    /// Returns a summary of symbols (functions, structs, etc.) with their signatures.
    async fn _build_lsp_context(&self, file_path: &str) -> Option<String> {
        // Use the lsp_document_symbols tool to get file structure
        let args = serde_json::json!({
            "file": file_path
        });

        match self.tools.execute("lsp_document_symbols", args).await {
            Ok(result) => {
                if let Some(symbols) = result.get("symbols").and_then(|s| s.as_array()) {
                    if symbols.is_empty() {
                        return None;
                    }

                    let mut context = format!("\n## Symbol Outline for `{}`:\n", file_path);
                    for sym in symbols.iter().take(50) {
                        // Limit to prevent context bloat
                        if let (Some(name), Some(kind), Some(line)) = (
                            sym.get("name").and_then(|n| n.as_str()),
                            sym.get("kind").and_then(|k| k.as_str()),
                            sym.get("line").and_then(|l| l.as_u64()),
                        ) {
                            context.push_str(&format!("- {} `{}` (line {})\n", kind, name, line));
                        }
                    }
                    return Some(context);
                }
                None
            }
            Err(e) => {
                tracing::debug!("LSP context building failed for {}: {}", file_path, e);
                None
            }
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
            "tool_search",
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

    /// Shared acknowledgement flag from the ESC listener pause handshake.
    pub(crate) fn esc_pause_ack_token(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.esc_pause_ack)
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

    // === Plan Mode Manager API ===

    /// Enter structured plan mode - restricts tools to read-only
    pub fn enter_plan_mode(&mut self) {
        self.plan_mode_manager.enter_plan_mode();
        info!("Entered plan mode - only read-only tools allowed");
    }

    /// Exit structured plan mode - return to normal execution
    pub fn exit_plan_mode(&mut self) {
        self.plan_mode_manager.exit_plan_mode();
        info!("Exited plan mode");
    }

    /// Check if in structured plan mode (planning or executing)
    pub fn is_in_plan_mode(&self) -> bool {
        self.plan_mode_manager.is_in_plan_mode()
    }

    /// Check if currently in the planning phase (before approval)
    pub fn is_planning_phase(&self) -> bool {
        self.plan_mode_manager.is_planning()
    }

    /// Approve the current plan and switch to executing
    pub fn approve_plan(&mut self) {
        self.plan_mode_manager.approve_plan();
        info!("Plan approved - switching to execution");
    }

    /// Store a structured plan
    pub fn store_plan(&mut self, plan: plan_mode::Plan) {
        self.plan_mode_manager.store_plan(plan);
    }

    /// Get the current plan
    pub fn get_plan(&self) -> Option<&plan_mode::Plan> {
        self.plan_mode_manager.get_plan()
    }

    /// Get the current plan text
    pub fn get_plan_text(&self) -> Option<&str> {
        self.plan_mode_manager.get_plan_text()
    }

    /// Check if the current plan is approved
    pub fn is_plan_approved(&self) -> bool {
        self.plan_mode_manager.is_approved()
    }

    /// Clear the current plan and exit plan mode
    pub fn clear_plan(&mut self) {
        self.plan_mode_manager.clear_plan();
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

    // ========================================================================
    // Three-Layer Context Compression Methods
    // ========================================================================

    /// Run MicroCompact - fast local compression with no API call
    pub fn compact_micro(&mut self) -> compression::CompressionMetrics {
        let metrics = self.compression_orchestrator.run_micro(&mut self.messages);
        info!("MicroCompact: {}", metrics.summary());
        metrics
    }

    /// Run AutoCompact - LLM-based summarization
    pub async fn compact_auto(&mut self) -> anyhow::Result<compression::CompressionMetrics> {
        let metrics = self
            .compression_orchestrator
            .run_auto(&self.client, &mut self.messages)
            .await?;
        info!("AutoCompact: {}", metrics.summary());
        Ok(metrics)
    }

    /// Run FullCompact - nuclear option with file re-injection
    pub async fn compact_full(&mut self) -> anyhow::Result<compression::CompressionMetrics> {
        let metrics = self
            .compression_orchestrator
            .run_full(&self.client, &mut self.messages)
            .await?;
        info!("FullCompact: {}", metrics.summary());
        Ok(metrics)
    }

    /// Run compression based on current context usage
    pub async fn compact_auto_trigger(&mut self) -> Option<compression::CompressionMetrics> {
        let current_tokens = self.total_tokens_used();
        let context_window = self.max_context_tokens;

        self.compression_orchestrator
            .check_and_compress(
                &self.client,
                &mut self.messages,
                current_tokens,
                context_window,
            )
            .await
    }

    /// Record a file access for FullCompact re-injection
    pub fn record_file_access(&mut self, path: &str) {
        self.compression_orchestrator.record_file_access(path);
    }

    /// Get compression statistics
    pub fn compression_stats(&self) -> String {
        let total_saved = self.compression_orchestrator.total_tokens_saved();
        let history = self.compression_orchestrator.metrics_history();

        let mut stats = format!(
            "Total tokens saved: {}\nCompression operations: {}\n",
            total_saved,
            history.len()
        );

        if let Some(last) = history.last() {
            stats.push_str(&format!("\nLast compression:\n  {}", last.summary()));
        }

        let recent_files = self
            .compression_orchestrator
            .file_tracker()
            .get_recent_files(5);
        if !recent_files.is_empty() {
            stats.push_str("\n\nRecently accessed files:\n");
            for (i, path) in recent_files.iter().enumerate() {
                stats.push_str(&format!("  {}. {}\n", i + 1, path));
            }
        }

        stats
    }

    /// Get the compression orchestrator (for advanced usage)
    pub fn compression_orchestrator(&self) -> &CompressionOrchestrator {
        &self.compression_orchestrator
    }

    /// Get mutable access to the compression orchestrator
    pub fn compression_orchestrator_mut(&mut self) -> &mut CompressionOrchestrator {
        &mut self.compression_orchestrator
    }

    /// Prompt the user for permission to execute a tool.
    ///
    /// This method handles interactive prompting for tool execution permission.
    /// It supports both TUI and CLI modes, and provides options for:
    /// - Yes: Allow this invocation
    /// - No: Deny this invocation
    /// - Always: Remember choice for this session (adds to permission_store)
    /// - Yolo: Switch execution mode to Yolo for the rest of the session
    ///
    /// In non-interactive mode, returns an error suggesting --yolo mode.
    pub async fn prompt_for_permission(
        &self,
        tool_name: &str,
        reason: &str,
    ) -> Result<PermissionPromptResult> {
        // Check if we're in non-interactive mode
        if !self.is_interactive() {
            return Err(anyhow::anyhow!(
                "Tool '{}' requires confirmation: {}. \
                 Run with --yolo flag or change execution mode to allow this operation.",
                tool_name,
                reason
            ));
        }

        // Check if TUI is active - if so, we need to handle differently
        if self.has_tui_renderer() {
            // For TUI mode, emit an event and wait for user response
            // The TUI will handle displaying the prompt and sending back the response
            self.emit_event(AgentEvent::PermissionRequested {
                tool_name: tool_name.to_string(),
                reason: reason.to_string(),
            });

            // In TUI mode, we need to wait for the user response via a different mechanism
            // For now, fall back to CLI prompt by temporarily suspending TUI
            return self.prompt_for_permission_cli(tool_name, reason).await;
        }

        // CLI interactive mode
        self.prompt_for_permission_cli(tool_name, reason).await
    }

    /// CLI-based permission prompt (used for both CLI and TUI fallback)
    async fn prompt_for_permission_cli(
        &self,
        tool_name: &str,
        reason: &str,
    ) -> Result<PermissionPromptResult> {
        use colored::Colorize;
        use std::io::{self, Write};

        eprintln!();
        eprintln!(
            "{} Tool '{}' requires confirmation",
            "⚠️ ".bright_yellow(),
            tool_name.bright_cyan()
        );
        eprintln!("  Reason: {}", reason);
        eprintln!();
        eprint!("  Allow? [Y]es / [N]o / [A]lways / Y[o]lo mode: ");
        io::stderr().flush()?;

        // Read user input asynchronously while pausing the ESC listener so it
        // does not compete for stdin events.
        let input = execution::read_line_pausing_esc(&self.esc_paused, &self.esc_pause_ack).await?;

        match input.trim().to_lowercase().as_str() {
            "y" | "yes" => {
                eprintln!("  {} Allowed.", "✓".bright_green());
                Ok(PermissionPromptResult::Yes)
            }
            "n" | "no" => {
                eprintln!("  {} Denied.", "✗".bright_red());
                Ok(PermissionPromptResult::No)
            }
            "a" | "always" => {
                eprintln!("  {} Always allowed for this session.", "✓".bright_green());
                Ok(PermissionPromptResult::Always)
            }
            "o" | "yolo" => {
                eprintln!(
                    "  {} Switching to YOLO mode for this session.",
                    "⚡".bright_red()
                );
                // Emit event to request mode change - the agent loop will handle this
                self.emit_event(AgentEvent::ModeChangeRequested {
                    mode: crate::config::ExecutionMode::Yolo,
                });
                Ok(PermissionPromptResult::Yolo)
            }
            _ => {
                eprintln!("  {} Invalid choice, denying.", "✗".bright_red());
                Ok(PermissionPromptResult::No)
            }
        }
    }

    // ========================================================================
    // FailureMode accessors
    //
    // These are read-only views into the per-task counters that the
    // failure-mode classifier consumes at run end. They are intentionally
    // narrow: each one returns a single primitive so the classifier never
    // needs to know about internal field names.
    // ========================================================================

    /// Number of successful mutating tool calls in the current task.
    pub fn mutating_tool_call_count(&self) -> usize {
        self.mutating_tool_call_count
    }

    /// Total tool calls attempted (success + failure) in the current task.
    pub fn total_tool_call_count(&self) -> usize {
        self.total_tool_call_count
    }

    /// How many times the read-only progress guard fired in the current task.
    pub fn progress_guard_fire_count(&self) -> usize {
        self.progress_guard_fire_count
    }

    /// Number of consecutive prose-only turns (no tool call) at run end.
    /// Mirrors `consecutive_no_action_prompts`, which measures the same thing.
    pub fn consecutive_no_tool_call_turns(&self) -> usize {
        self.consecutive_no_action_prompts
    }

    /// Count of tool calls that have been hard-blocked after repeated failures.
    ///
    /// `note_permanently_blocked` deduplicates on write, so the stored vector
    /// already contains unique blocked tool names and its length is the honest
    /// count surfaced to `FailureMode` and the CLI banner.
    pub fn permanently_blocked_tool_calls_len(&self) -> usize {
        self.permanently_blocked_tool_calls.len()
    }

    /// Cumulative token usage across all LLM calls in the current task.
    pub fn cumulative_token_usage(&self) -> &crate::observability::dashboard::TokenUsage {
        &self.cumulative_token_usage
    }

    /// Failure mode from the most recent run.
    pub fn last_run_failure_mode(&self) -> Option<&failure_mode::FailureMode> {
        self.last_run_failure_mode.as_ref()
    }

    /// Current loop iteration count (number of turns through the agent loop).
    pub fn current_iteration(&self) -> usize {
        self.loop_control.current_iteration()
    }

    /// The model name configured for this agent.
    pub fn model(&self) -> &str {
        &self.config.model
    }

    /// Count of HTTP 400 "Assistant response prefill incompatible" responses.
    pub fn prefill_400_count(&self) -> usize {
        self.prefill_400_count
    }

    /// Whether the prefill circuit breaker is currently open.
    pub fn prefill_breaker_open(&self) -> bool {
        self.prefill_breaker_open
    }

    /// Length in bytes of the last assistant response. Used by the
    /// classifier to surface "47KB of prose without a tool call".
    pub fn last_assistant_response_len(&self) -> usize {
        self.last_assistant_response.len()
    }

    /// True if the last assistant response contains a "Final answer" marker
    /// — a tell-tale of FakeComplete when no mutating call ever happened.
    pub fn last_assistant_response_has_final_answer(&self) -> bool {
        let lower = self.last_assistant_response.to_lowercase();
        lower.contains("final answer")
    }

    /// True when the current task description has been classified as one
    /// that requires file mutation (`fix`, `implement`, `edit`, `add`, etc.).
    /// Used by `FailureMode::classify` to flag suspicious natural-completion
    /// runs where the model wrote zero files but claimed success.
    pub fn current_task_requires_mutation(&self) -> bool {
        tool_dispatch::task_requires_mutation(self.task_context_for_classification())
    }

    /// Reset all per-task failure-mode counters. Called when starting or
    /// resetting a task so counters from a prior run never leak into the
    /// next classification.
    pub(super) fn reset_failure_mode_counters(&mut self) {
        self.mutating_tool_call_count = 0;
        self.total_tool_call_count = 0;
        self.mutation_gate_rejections = 0;
        self.consecutive_stale_verification = 0;
        self.progress_guard_fire_count = 0;
        self.mutation_sequence = 0;
        self.last_successful_verification_mutation_sequence = 0;
        self.last_failed_verification_summary = None;
        self.permanently_blocked_tool_calls.clear();
        self.prefill_400_count = 0;
        self.prefill_breaker_open = false;
    }

    /// Increment the mutating-call counter. Called from tool dispatch when
    /// `file_write` / `file_edit` / `file_delete` / `shell_exec` (with mutation)
    /// completes successfully.
    pub(super) fn note_mutating_tool_call(&mut self) {
        self.mutating_tool_call_count += 1;
        self.mutation_sequence += 1;
        self.mutation_gate_rejections = 0;
        self.consecutive_stale_verification = 0;
        self.last_failed_verification_summary = None;
        self.post_edit_observational_shell_count = 0;
    }

    /// Increment the total tool-call counter. Should be called for every
    /// dispatched tool call regardless of success.
    pub(super) fn note_total_tool_call(&mut self) {
        self.total_tool_call_count += 1;
    }

    /// Note that the read-only progress guard fired. Called by
    /// `maybe_block_progressless_batch` whenever it emits the PROGRESS GUARD
    /// rejection.
    pub(super) fn note_progress_guard_fired(&mut self) {
        self.progress_guard_fire_count += 1;
    }

    /// Record that a tool call was permanently blocked after repeated retries.
    ///
    /// Dedupes by exact entry so the same signature does not inflate the
    /// stored vec on every retry attempt.  The cap stays at 64 to bound
    /// memory in pathological loops.
    pub(super) fn note_permanently_blocked(&mut self, tool_name: &str) {
        if !self
            .permanently_blocked_tool_calls
            .iter()
            .any(|s| s == tool_name)
            && self.permanently_blocked_tool_calls.len() < 64
        {
            self.permanently_blocked_tool_calls
                .push(tool_name.to_string());
        }
    }

    /// Record an "Assistant response prefill incompatible" 400 from the API.
    pub(super) fn note_prefill_400(&mut self) {
        self.prefill_400_count = self.prefill_400_count.saturating_add(1);
        if self.prefill_400_count >= 3 {
            self.prefill_breaker_open = true;
        }
    }

    // ===== test-only setters used by failure_mode unit tests =====

    #[cfg(test)]
    pub(super) fn test_set_mutating_count(&mut self, n: usize) {
        self.mutating_tool_call_count = n;
    }
    #[cfg(test)]
    pub(super) fn test_set_total_tool_calls(&mut self, n: usize) {
        self.total_tool_call_count = n;
    }
    #[cfg(test)]
    pub(super) fn test_set_progress_guard_fires(&mut self, n: usize) {
        self.progress_guard_fire_count = n;
    }
    #[cfg(test)]
    pub(super) fn test_set_consecutive_no_action(&mut self, n: usize) {
        self.consecutive_no_action_prompts = n;
    }
    #[cfg(test)]
    pub(super) fn test_set_permanently_blocked(&mut self, n: usize) {
        self.permanently_blocked_tool_calls = (0..n).map(|i| format!("tool_{}", i)).collect();
    }
    #[cfg(test)]
    pub(super) fn test_set_prefill_400s(&mut self, n: usize) {
        self.prefill_400_count = n;
    }
    #[cfg(test)]
    pub(super) fn test_set_prefill_breaker_open(&mut self, v: bool) {
        self.prefill_breaker_open = v;
    }
    #[cfg(test)]
    pub(super) fn test_set_last_assistant_response(&mut self, s: String) {
        self.last_assistant_response = s;
    }
}

/// Result of a permission prompt
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PermissionPromptResult {
    /// User approved this invocation
    Yes,
    /// User denied this invocation
    No,
    /// User approved and wants to always allow this tool for this session
    Always,
    /// User wants to switch to YOLO mode
    Yolo,
}

impl PermissionPromptResult {
    /// Returns true if the operation should proceed
    pub fn is_allowed(&self) -> bool {
        matches!(
            self,
            PermissionPromptResult::Yes
                | PermissionPromptResult::Always
                | PermissionPromptResult::Yolo
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum PendingMessageOrigin {
    InteractiveQueue,
    ManualQueue,
}

#[derive(Debug, Clone)]
pub(super) struct PendingMessage {
    pub content: String,
    pub queued_at: Instant,
    pub origin: PendingMessageOrigin,
}

impl PendingMessage {
    pub(super) fn new(
        content: impl Into<String>,
        origin: PendingMessageOrigin,
        queued_at: Instant,
    ) -> Self {
        Self {
            content: content.into(),
            queued_at,
            origin,
        }
    }
}

#[cfg(test)]
mod tests;
