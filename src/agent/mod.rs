use anyhow::{Context, Result};
use colored::*;
use serde_json::Value;
use std::collections::{HashSet, VecDeque};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::Instant;
use tracing::{debug, info, warn};

use crate::analyzer::ErrorAnalyzer;
use crate::api::types::{Message, ToolCall};
use crate::api::{ApiClient, StreamChunk, ThinkingMode};
use crate::checkpoint::{CheckpointManager, TaskCheckpoint};
use crate::cognitive::{CognitiveState, CyclePhase};
use crate::config::Config;
use crate::memory::AgentMemory;
use crate::output;
use crate::safety::SafetyChecker;
#[cfg(feature = "resilience")]
use crate::self_healing::{SelfHealingConfig, SelfHealingEngine};
use crate::session::chat_store::ChatStore;
use crate::session::edit_history::EditHistory;
use crate::telemetry::{enter_agent_step, record_state_transition};
use crate::tools::ToolRegistry;
use crate::verification::{VerificationConfig, VerificationGate};

mod checkpointing;
pub mod context;
mod execution;
mod interactive;
pub mod loop_control;
pub mod planning;

use context::ContextCompressor;
use loop_control::{AgentLoop, AgentState};
use planning::Planner;

/// Agent-specific errors that require special handling
#[derive(Debug, Clone, PartialEq)]
pub enum AgentError {
    /// Tool requires confirmation but running in non-interactive mode
    ConfirmationRequired { tool_name: String },
}

impl std::fmt::Display for AgentError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AgentError::ConfirmationRequired { tool_name } => write!(
                f,
                "Tool '{}' requires confirmation but running in non-interactive mode. \
                Use --yolo to auto-approve tools, or run interactively.",
                tool_name
            ),
        }
    }
}

impl std::error::Error for AgentError {}

/// Check if an anyhow error is a confirmation-required error (fatal in non-interactive mode)
fn is_confirmation_error(e: &anyhow::Error) -> bool {
    e.downcast_ref::<AgentError>()
        .map(|ae| matches!(ae, AgentError::ConfirmationRequired { .. }))
        .unwrap_or(false)
}

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
    /// Self-healing engine for automatic recovery attempts
    #[cfg(feature = "resilience")]
    self_healing: SelfHealingEngine,
}

impl Agent {
    pub async fn new(config: Config) -> Result<Self> {
        let client = ApiClient::new(&config)?;
        let tools = ToolRegistry::new();
        let memory = AgentMemory::new(&config)?;
        let safety = SafetyChecker::new(&config.safety);
        let loop_control = AgentLoop::new(config.agent.max_iterations);
        let compressor = ContextCompressor::new(config.max_tokens);

        // Choose between native function calling or XML-based tool parsing
        let system_prompt = if config.agent.native_function_calling {
            // Native function calling: simple prompt, tools passed via API
            info!("Using native function calling mode");
            r#"You are Selfware, an expert software engineering AI assistant.

You have access to tools for file operations, git, cargo, shell commands, and more.
Use tools to accomplish tasks step by step. Verify each step succeeded before proceeding.
When editing files, include 3-5 lines of context for unique matches.
Run cargo_check after code changes to verify compilation.
When the task is complete, respond with a summary of what was done."#
                .to_string()
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

## Guidelines
1. Use <name>TOOL_NAME</name> - never <function>
2. Arguments must be valid JSON inside <arguments>...</arguments>
3. Each <tool>...</tool> block is executed separately
4. Wait for tool results before proceeding
5. When done, respond with plain text only (no tool tags)"#,
                tool_descriptions
            )
        };

        let messages = vec![Message::system(system_prompt)];

        // Initialize checkpoint manager if configured
        let checkpoint_manager = CheckpointManager::default_path().ok();

        // Initialize cognitive state
        let cognitive_state = CognitiveState::new();

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
            verification_gate,
            error_analyzer,
            context_files: Vec::new(),
            stale_files: HashSet::new(),
            last_checkpoint_persisted_at: Instant::now(),
            last_checkpoint_tool_calls: 0,
            checkpoint_persisted_once: false,
            edit_history,
            last_assistant_response: String::new(),
            chat_store,
            cancelled: Arc::new(AtomicBool::new(false)),
            pending_messages: VecDeque::new(),
            #[cfg(feature = "resilience")]
            self_healing,
        })
    }

    /// Get tools for API calls - returns Some(tools) if native function calling is enabled
    fn api_tools(&self) -> Option<Vec<crate::api::types::ToolDefinition>> {
        if self.config.agent.native_function_calling {
            Some(self.tools.definitions())
        } else {
            None
        }
    }

    /// Extract function name from a tool_call XML block for clean display
    fn extract_tool_name(xml: &str) -> Option<String> {
        // Match <function=name> or <function>name pattern
        if let Some(start) = xml.find("<function=") {
            let rest = &xml[start + "<function=".len()..];
            let end = rest.find(['>', '<', '\n']).unwrap_or(rest.len());
            let name = rest[..end].trim();
            if !name.is_empty() {
                return Some(name.to_string());
            }
        }
        // Also try <function>name</function> pattern
        if let Some(start) = xml.find("<function>") {
            let rest = &xml[start + "<function>".len()..];
            if let Some(end) = rest.find("</function>") {
                let name = rest[..end].trim();
                if !name.is_empty() {
                    return Some(name.to_string());
                }
            }
        }
        None
    }

    /// Chat with streaming, displaying output as it arrives
    /// Returns (content, reasoning, tool_calls) tuple
    async fn chat_streaming(
        &self,
        messages: Vec<Message>,
        tools: Option<Vec<crate::api::types::ToolDefinition>>,
        thinking: ThinkingMode,
    ) -> Result<(String, Option<String>, Option<Vec<ToolCall>>)> {
        use std::io::{self, Write};

        // Start loading spinner with a random phrase while waiting for first token
        let mut spinner = Some(crate::ui::spinner::TerminalSpinner::start(
            crate::ui::loading_phrases::random_phrase(),
        ));
        let mut phrase_rotation = tokio::time::Instant::now();

        let stream = self.client.chat_stream(messages, tools, thinking).await?;

        let mut rx = stream.into_channel().await;
        let mut content = String::new();
        let mut reasoning = String::new();
        let mut tool_calls: Vec<ToolCall> = Vec::new();
        let mut in_reasoning = false;
        let mut display_buf = String::new();
        let mut in_tool_tag = false;

        while let Some(chunk_result) = rx.recv().await {
            let chunk = chunk_result?;

            // Rotate loading phrase every 3 seconds while spinner is active
            if let Some(ref s) = spinner {
                if phrase_rotation.elapsed() > tokio::time::Duration::from_secs(3) {
                    s.set_message(crate::ui::loading_phrases::random_phrase());
                    phrase_rotation = tokio::time::Instant::now();
                }
            }

            match chunk {
                StreamChunk::Content(text) => {
                    // Stop spinner on first content
                    if let Some(s) = spinner.take() {
                        drop(s);
                    }
                    if in_reasoning {
                        // Finished reasoning, now showing content
                        in_reasoning = false;
                        if !output::is_compact() {
                            println!(); // End reasoning line
                        }
                    }
                    // Always accumulate full content for parsing
                    content.push_str(&text);

                    // Filter out <tool_call> XML blocks from display
                    // Buffer content and only print text outside tool_call tags
                    display_buf.push_str(&text);

                    // Process display buffer: suppress tool_call blocks
                    loop {
                        if in_tool_tag {
                            // We're inside a <tool_call> - look for closing tag
                            if let Some(end_pos) = display_buf.find("</tool_call>") {
                                let end = end_pos + "</tool_call>".len();
                                // Extract the tool call text to show a clean summary
                                let tool_xml = &display_buf[..end];
                                if let Some(fname) = Self::extract_tool_name(tool_xml) {
                                    print!("  {} {}...", "🔧".dimmed(), fname.bright_cyan());
                                    io::stdout().flush().ok();
                                }
                                display_buf = display_buf[end..].to_string();
                                in_tool_tag = false;
                            } else {
                                break; // Wait for more data
                            }
                        } else {
                            // Look for start of <tool_call>
                            if let Some(start_pos) = display_buf.find("<tool_call>") {
                                // Print everything before the tag
                                let before = &display_buf[..start_pos];
                                if !before.is_empty() {
                                    print!("{}", before);
                                    io::stdout().flush().ok();
                                }
                                display_buf = display_buf[start_pos..].to_string();
                                in_tool_tag = true;
                            } else if display_buf.contains('<') && !display_buf.contains('>') {
                                // Partial tag at end - buffer it
                                break;
                            } else {
                                // No tags - print everything
                                if !display_buf.is_empty() {
                                    print!("{}", display_buf);
                                    io::stdout().flush().ok();
                                }
                                display_buf.clear();
                                break;
                            }
                        }
                    }
                }
                StreamChunk::Reasoning(text) => {
                    // Stop spinner on first reasoning
                    if let Some(s) = spinner.take() {
                        drop(s);
                    }
                    if !output::is_compact() {
                        if !in_reasoning {
                            in_reasoning = true;
                            output::thinking_prefix();
                        }
                        output::thinking(&text, true);
                        io::stdout().flush().ok();
                    }
                    reasoning.push_str(&text);
                }
                StreamChunk::ToolCall(call) => {
                    tool_calls.push(call);
                }
                StreamChunk::Usage(u) => {
                    debug!(
                        "Token usage: {} prompt, {} completion",
                        u.prompt_tokens, u.completion_tokens
                    );
                    output::record_tokens(u.prompt_tokens as u64, u.completion_tokens as u64);
                    output::print_token_usage(u.prompt_tokens as u64, u.completion_tokens as u64);
                }
                StreamChunk::Done => break,
            }
        }

        // Flush any remaining display buffer (non-tool-call text)
        if !display_buf.is_empty() && !in_tool_tag {
            print!("{}", display_buf);
            io::stdout().flush().ok();
        }

        // Ensure we end with a newline if we printed content
        if !content.is_empty() || !reasoning.is_empty() {
            println!();
        }

        Ok((
            content,
            if reasoning.is_empty() {
                None
            } else {
                Some(reasoning)
            },
            if tool_calls.is_empty() {
                None
            } else {
                Some(tool_calls)
            },
        ))
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

    /// Cycle to next execution mode (for Shift+Tab switching)
    pub fn cycle_execution_mode(&mut self) -> crate::config::ExecutionMode {
        use crate::config::ExecutionMode;
        self.config.execution_mode = match self.config.execution_mode {
            ExecutionMode::Normal => ExecutionMode::AutoEdit,
            ExecutionMode::AutoEdit => ExecutionMode::Yolo,
            ExecutionMode::Yolo => ExecutionMode::Normal,
            ExecutionMode::Daemon => ExecutionMode::Normal, // Daemon can't be cycled to
        };
        self.config.execution_mode
    }

    /// Check if tool execution needs confirmation based on current mode and risk level
    pub fn needs_confirmation(&self, tool_name: &str) -> bool {
        use crate::config::ExecutionMode;

        // Read-only tools never need confirmation
        let safe_tools = [
            "file_read",
            "directory_tree",
            "glob_find",
            "grep_search",
            "git_status",
            "git_diff",
            "git_log",
            "ripgrep_search",
            "web_search",
        ];

        if safe_tools.contains(&tool_name) {
            return false;
        }

        match self.config.execution_mode {
            ExecutionMode::Yolo | ExecutionMode::Daemon => false, // Never ask
            ExecutionMode::AutoEdit => {
                // Auto-approve file operations, ask for destructive operations
                !matches!(
                    tool_name,
                    "file_write" | "file_edit" | "file_create" | "directory_tree" | "glob_find"
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

    /// Shared cancellation token for Ctrl+C interrupt handling.
    pub(crate) fn cancel_token(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.cancelled)
    }

    /// True when the current task should stop as soon as possible.
    pub(crate) fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Relaxed)
    }

    /// Clear cancellation state after handling an interrupt.
    pub(crate) fn reset_cancellation(&self) {
        self.cancelled.store(false, Ordering::Relaxed);
    }

    // =========================================================================
    // Context Management
    // =========================================================================

    /// Estimate total tokens from accumulated messages (the actual context sent to API)
    fn estimate_messages_tokens(&self) -> usize {
        self.messages
            .iter()
            .map(|m| crate::token_count::estimate_tokens_with_overhead(&m.content, 4))
            .sum()
    }

    /// Get the best estimate of total tokens used
    fn total_tokens_used(&self) -> usize {
        // Use the MAX of: API-reported usage, message estimates, memory estimates
        // API usage may be 0 if the provider doesn't send usage chunks
        let (api_prompt, api_completion) = output::get_total_tokens();
        let api_tokens = (api_prompt + api_completion) as usize;
        let msg_tokens = self.estimate_messages_tokens();
        let mem_tokens = self.memory.total_tokens();
        api_tokens.max(msg_tokens).max(mem_tokens)
    }

    fn context_usage_pct(&self) -> f64 {
        let tokens = self.total_tokens_used();
        let window = self.memory.context_window();
        if window == 0 {
            return 0.0;
        }
        (tokens as f64 / window as f64 * 100.0).min(100.0)
    }

    /// Print a Qwen Code-style status bar line before the prompt
    ///
    /// Layout: `  ? for shortcuts                            45.2% context used`
    fn print_status_bar(&self) {
        use colored::*;

        let pct = self.context_usage_pct();
        let tokens = self.total_tokens_used();
        let window = self.memory.context_window();
        let (k_tokens, k_window) = (tokens as f64 / 1000.0, window as f64 / 1000.0);

        // Build progress bar (10 chars wide)
        let bar_width = 10;
        let filled = ((pct / 100.0) * bar_width as f64) as usize;
        let bar: String = (0..bar_width)
            .map(|i| if i < filled { "█" } else { "░" })
            .collect();

        // Color the bar based on usage
        let colored_bar = if pct > 90.0 {
            bar.bright_red()
        } else if pct > 70.0 {
            bar.bright_yellow()
        } else {
            bar.bright_green()
        };

        // Get cost from actual API usage
        let cost = tokens as f64 * 0.000003; // rough estimate

        // Model name
        let model_name = &self.config.model;
        let short_model = if model_name.len() > 15 {
            &model_name[..15]
        } else {
            model_name
        };

        // Mode indicator
        let mode = match self.execution_mode() {
            crate::config::ExecutionMode::Normal => "normal",
            crate::config::ExecutionMode::AutoEdit => "auto-edit",
            crate::config::ExecutionMode::Yolo => "YOLO",
            crate::config::ExecutionMode::Daemon => "daemon",
        };

        // Terminal width for alignment
        let term_width = crossterm::terminal::size()
            .map(|(w, _)| w as usize)
            .unwrap_or(80);

        // Left side: mode + hint
        let left = format!("[{}] ? for shortcuts", mode);
        // Right side: bar + percentage + tokens + cost
        let right = format!(
            "{} {:.1}% ({:.1}k/{:.0}k) ${:.2} [{}]",
            bar, pct, k_tokens, k_window, cost, short_model
        );

        // Pad middle with spaces
        let padding = if left.len() + right.len() + 2 < term_width {
            term_width - left.len() - right.len() - 2
        } else {
            1
        };

        // Print colored version
        let mode_colored = match self.execution_mode() {
            crate::config::ExecutionMode::Yolo => format!("[{}]", mode).bright_red(),
            crate::config::ExecutionMode::AutoEdit => format!("[{}]", mode).bright_yellow(),
            _ => format!("[{}]", mode).bright_cyan(),
        };

        println!(
            " {} {}{}  {} {:.1}% ({:.1}k/{:.0}k) {} [{}]",
            mode_colored,
            "? for shortcuts".dimmed(),
            " ".repeat(padding),
            colored_bar,
            pct,
            k_tokens,
            k_window,
            format!("${:.2}", cost).dimmed(),
            short_model.dimmed(),
        );
    }

    /// Show compact startup context line (Claude Code style)
    fn show_startup_context(&self) {
        let tokens = self.total_tokens_used();
        let window = self.memory.context_window();
        let used_pct = (tokens as f64 / window as f64 * 100.0).min(100.0);
        let tool_count = self.tools.list().len();
        let cwd = std::env::current_dir()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|_| ".".to_string());
        let short_cwd = if cwd.len() > 40 {
            format!("...{}", &cwd[cwd.len() - 37..])
        } else {
            cwd
        };

        let model_name = &self.config.model;
        let short_model = if model_name.len() > 20 {
            &model_name[..20]
        } else {
            model_name
        };

        let (k_tokens, k_window) = (tokens as f64 / 1000.0, window as f64 / 1000.0);

        println!(
            "  {} {}  {} {:.1}k/{:.0}k ({:.0}%)  {} {}  {} {}",
            "Model:".dimmed(),
            short_model.bright_cyan(),
            "Context:".dimmed(),
            k_tokens,
            k_window,
            used_pct,
            "Tools:".dimmed(),
            tool_count.to_string().bright_white(),
            "Dir:".dimmed(),
            short_cwd.bright_white(),
        );
    }

    /// Show context statistics with visual progress bar
    fn show_context_stats(&self) {
        let tokens = self.total_tokens_used();
        let window = self.memory.context_window();
        let used_pct = (tokens as f64 / window as f64 * 100.0).min(100.0);
        let messages = self.messages.len();
        let memory_entries = self.memory.len();
        let available = window.saturating_sub(tokens);
        let files_loaded = self.context_files.len();

        // Build progress bar with gradient effect
        let bar_width = 32;
        let filled = ((used_pct / 100.0) * bar_width as f64) as usize;

        // Determine health status
        let (status_icon, status_text, bar_char) = if used_pct > 90.0 {
            ("🔴", "CRITICAL", "▓")
        } else if used_pct > 70.0 {
            ("🟡", "WARNING ", "▒")
        } else if used_pct > 50.0 {
            ("🟢", "HEALTHY ", "░")
        } else {
            ("🟢", "OPTIMAL ", "░")
        };

        let bar: String = (0..bar_width)
            .map(|i| {
                if i < filled {
                    if used_pct > 90.0 {
                        "█"
                    } else if used_pct > 70.0 {
                        "▓"
                    } else {
                        "▒"
                    }
                } else {
                    bar_char
                }
            })
            .collect();

        // Check if colors are enabled (respects --no-color and NO_COLOR env)
        let colors_enabled = colored::control::SHOULD_COLORIZE.should_colorize();

        // Rusty, weathered color palette - like oxidized metal under salty water
        let (rust, rust_light, patina, patina_light, sand, worn, coral, aged, reset) =
            if colors_enabled {
                (
                    "\x1b[38;5;130m", // Deep rust orange
                    "\x1b[38;5;173m", // Light copper/rust
                    "\x1b[38;5;66m",  // Oxidized teal/verdigris
                    "\x1b[38;5;109m", // Weathered blue-green
                    "\x1b[38;5;180m", // Faded sandy gold
                    "\x1b[38;5;245m", // Weathered gray
                    "\x1b[38;5;174m", // Faded coral/salmon
                    "\x1b[38;5;137m", // Aged brown
                    "\x1b[0m",        // Reset
                )
            } else {
                ("", "", "", "", "", "", "", "", "")
            };

        // Progress bar colors - rusty theme
        let bar_color = if !colors_enabled {
            ""
        } else if used_pct > 90.0 {
            "\x1b[38;5;160m" // Deep warning red
        } else if used_pct > 70.0 {
            "\x1b[38;5;172m" // Amber rust
        } else {
            "\x1b[38;5;108m" // Weathered sage green
        };

        println!();
        println!(
            "  {}┌─────────────────────────────────────────────────────────────┐{}",
            patina, reset
        );
        println!(
            "  {}│{}                                                             {}│{}",
            patina, reset, patina, reset
        );
        println!("  {}│{}   {}███████╗███████╗██╗     ███████╗██╗    ██╗ █████╗ ██████╗ ███████╗{}  {}│{}", patina, reset, rust, reset, patina, reset);
        println!("  {}│{}   {}██╔════╝██╔════╝██║     ██╔════╝██║    ██║██╔══██╗██╔══██╗██╔════╝{}  {}│{}", patina, reset, rust_light, reset, patina, reset);
        println!("  {}│{}   {}███████╗█████╗  ██║     █████╗  ██║ █╗ ██║███████║██████╔╝█████╗  {} {}│{}", patina, reset, rust, reset, patina, reset);
        println!("  {}│{}   {}╚════██║██╔══╝  ██║     ██╔══╝  ██║███╗██║██╔══██║██╔══██╗██╔══╝  {} {}│{}", patina, reset, rust_light, reset, patina, reset);
        println!("  {}│{}   {}███████║███████╗███████╗██║     ╚███╔███╔╝██║  ██║██║  ██║███████╗{}  {}│{}", patina, reset, rust, reset, patina, reset);
        println!("  {}│{}   {}╚══════╝╚══════╝╚══════╝╚═╝      ╚══╝╚══╝ ╚═╝  ╚═╝╚═╝  ╚═╝╚══════╝{}  {}│{}", patina, reset, rust_light, reset, patina, reset);
        println!(
            "  {}│{}                        {}· w i n d o w ·{}                         {}│{}",
            patina, reset, patina_light, reset, patina, reset
        );
        println!(
            "  {}├─────────────────────────────────────────────────────────────┤{}",
            patina, reset
        );
        println!(
            "  {}│{}                                                             {}│{}",
            patina, reset, patina, reset
        );
        println!(
            "  {}│{}     {} {}{:<34}{} {:>5.1}% {}{}      {}│{}",
            patina,
            reset,
            status_icon,
            bar_color,
            bar,
            reset,
            used_pct,
            status_text,
            reset,
            patina,
            reset
        );
        println!(
            "  {}│{}                                                             {}│{}",
            patina, reset, patina, reset
        );
        println!(
            "  {}├─────────────────────────────────────────────────────────────┤{}",
            patina, reset
        );
        println!(
            "  {}│{}     {}⚓{}  {}tokens{}        {}{:>10}{} / {}{:>10}{}                  {}│{}",
            patina,
            reset,
            coral,
            reset,
            worn,
            reset,
            sand,
            tokens,
            reset,
            worn,
            window,
            reset,
            patina,
            reset
        );
        println!(
            "  {}│{}     {}◈{}  {}available{}     {}{:>10}{} tokens                       {}│{}",
            patina, reset, coral, reset, worn, reset, patina_light, available, reset, patina, reset
        );
        println!(
            "  {}├┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┤{}",
            patina, reset
        );
        println!(
            "  {}│{}     {}≋{}  {}messages{}      {}{:>10}{}                               {}│{}",
            patina, reset, coral, reset, worn, reset, aged, messages, reset, patina, reset
        );
        println!(
            "  {}│{}     {}◎{}  {}memory{}        {}{:>10}{} entries                      {}│{}",
            patina, reset, coral, reset, worn, reset, aged, memory_entries, reset, patina, reset
        );
        println!(
            "  {}│{}     {}⊡{}  {}files{}         {}{:>10}{} loaded                       {}│{}",
            patina, reset, coral, reset, worn, reset, aged, files_loaded, reset, patina, reset
        );
        println!(
            "  {}│{}                                                             {}│{}",
            patina, reset, patina, reset
        );
        println!(
            "  {}└─────────────────────────────────────────────────────────────┘{}",
            patina, reset
        );
        println!();
        println!(
            "      {}⚓ /ctx clear    ◈ /ctx load    ≋ /ctx reload    ⊡ /ctx copy{}",
            worn, reset
        );

        // Show tracked context files if any
        if !self.context_files.is_empty() {
            println!();
            println!("  {}📄 Context Files:{}", patina_light, reset);
            let mut total_file_tokens = 0usize;
            for path_str in &self.context_files {
                let file_tokens = self
                    .messages
                    .iter()
                    .find(|m| {
                        m.role == "user" && m.content.contains(&format!("// FILE: {}", path_str))
                    })
                    .map(|m| crate::token_count::estimate_tokens_with_overhead(&m.content, 4))
                    .unwrap_or(0);
                total_file_tokens += file_tokens;
                let is_stale = self.stale_files.contains(path_str);
                let stale_marker = if is_stale {
                    format!("  {}⟳ modified{}", coral, reset)
                } else {
                    String::new()
                };
                let k_tokens = file_tokens as f64 / 1000.0;
                println!(
                    "    {}→  {}{:>40}{}  {}({:.1}k tokens){}{}",
                    worn, sand, path_str, reset, worn, k_tokens, reset, stale_marker
                );
            }
            let total_k = total_file_tokens as f64 / 1000.0;
            println!(
                "  {}Total: {} files, {:.1}k tokens{}",
                aged,
                self.context_files.len(),
                total_k,
                reset
            );
        }

        if used_pct > 80.0 {
            println!(
                "  {} Context {:.0}% full - consider /compress or /ctx clear",
                "⚠".bright_yellow(),
                used_pct
            );
        }

        println!();
    }

    /// Refresh any stale files that are in context
    /// Returns the number of files refreshed
    fn refresh_stale_context_files(&mut self) -> usize {
        if self.stale_files.is_empty() {
            return 0;
        }

        // Find which stale files are in our context
        let stale_in_context: Vec<String> = self
            .context_files
            .iter()
            .filter(|f| self.stale_files.contains(f.as_str()))
            .cloned()
            .collect();

        if stale_in_context.is_empty() {
            self.stale_files.clear();
            return 0;
        }

        let mut refreshed = 0;
        for path_str in &stale_in_context {
            let file_marker = format!("// FILE: {}", path_str);
            if let Ok(content) = std::fs::read_to_string(path_str) {
                let file_header = format!(
                    "\n// ═══════════════════════════════════════════\n// FILE: {}\n// ═══════════════════════════════════════════\n",
                    path_str
                );
                let new_content = format!("{}{}", file_header, content);

                // Find and replace the existing message for this file
                if let Some(msg) = self
                    .messages
                    .iter_mut()
                    .find(|m| m.role == "user" && m.content.contains(&file_marker))
                {
                    msg.content = new_content;
                    refreshed += 1;
                }
            }
        }

        // Clear the stale set for refreshed files
        for path_str in &stale_in_context {
            self.stale_files.remove(path_str);
        }

        refreshed
    }

    /// Clear all context (messages and memory)
    fn clear_context(&mut self) {
        self.messages.retain(|m| m.role == "system");
        self.memory.clear();
        self.context_files.clear();
    }

    /// Load files matching pattern into context
    async fn load_files_to_context(&mut self, pattern: &str) -> Result<usize> {
        use std::fs;
        use walkdir::WalkDir;

        let mut loaded = 0;
        let mut total_tokens = 0usize;
        let extensions: Vec<&str> = if pattern == "." || pattern == "*" {
            vec!["rs", "toml", "md", "ts", "tsx", "js", "jsx", "py", "go"]
        } else {
            pattern
                .split(',')
                .map(|s| s.trim().trim_start_matches('.'))
                .collect()
        };

        println!();
        println!(
            "{} Loading files with extensions: {}",
            "📂".bright_cyan(),
            extensions.join(", ").bright_yellow()
        );
        println!();

        for entry in WalkDir::new(".")
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_file())
        {
            let path = entry.path();
            let path_str = path.display().to_string();

            // Skip build artifacts and hidden dirs
            if path_str.contains("/target/")
                || path_str.contains("/node_modules/")
                || path_str.contains("/.git/")
                || path_str.contains("/__pycache__/")
            {
                continue;
            }

            let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
            if extensions.contains(&ext) {
                if let Ok(content) = fs::read_to_string(path) {
                    let file_header = format!("\n// ═══════════════════════════════════════════\n// FILE: {}\n// ═══════════════════════════════════════════\n", path_str);
                    let full_content = format!("{}{}", file_header, content);
                    let file_tokens =
                        crate::token_count::estimate_tokens_with_overhead(&full_content, 4);
                    total_tokens += file_tokens;

                    // Add to context files tracking
                    if !self.context_files.contains(&path_str) {
                        self.context_files.push(path_str.clone());
                    }

                    // Add as user message with file content
                    self.messages.push(Message::user(full_content));

                    let k_tokens = file_tokens as f64 / 1000.0;
                    println!(
                        "  {} {} ({:.1}k tokens)",
                        "✓".bright_green(),
                        path_str.bright_white(),
                        k_tokens
                    );
                    loaded += 1;
                }
            }
        }

        let window = self.memory.context_window();
        let pct = if window > 0 {
            total_tokens as f64 / window as f64 * 100.0
        } else {
            0.0
        };
        let total_k = total_tokens as f64 / 1000.0;
        let window_k = window as f64 / 1000.0;
        println!();
        println!(
            "  {} Loaded {} files, ~{:.0}k tokens ({:.1}% of {:.0}k context)",
            "📊".bright_cyan(),
            loaded,
            total_k,
            pct,
            window_k
        );
        println!();
        Ok(loaded)
    }

    /// Reload previously loaded context files
    async fn reload_context(&mut self) -> Result<usize> {
        use std::fs;

        let files = self.context_files.clone();
        if files.is_empty() {
            println!(
                "{} No files previously loaded. Use '/ctx load <pattern>' first.",
                "⚠️".bright_yellow()
            );
            return Ok(0);
        }

        // Remove only messages that contain file content (// FILE: headers)
        // Keep all conversation messages intact
        self.messages
            .retain(|m| !(m.role == "user" && m.content.contains("// FILE: ")));

        let mut loaded = 0;
        for path_str in &files {
            if let Ok(content) = fs::read_to_string(path_str) {
                let file_header = format!("\n// ═══════════════════════════════════════════\n// FILE: {}\n// ═══════════════════════════════════════════\n", path_str);
                self.messages
                    .push(Message::user(format!("{}{}", file_header, content)));
                println!("  {} {}", "✓".bright_green(), path_str.bright_white());
                loaded += 1;
            }
        }

        // Clear stale tracking since we just refreshed everything
        self.stale_files.clear();

        Ok(loaded)
    }

    /// Copy all source files to clipboard
    fn copy_sources_to_clipboard(&self) -> Result<usize> {
        use std::fs;
        use std::process::{Command, Stdio};
        use walkdir::WalkDir;

        let mut output = String::new();
        let extensions = ["rs", "toml"];

        for entry in WalkDir::new(".")
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_file())
        {
            let path = entry.path();
            let path_str = path.display().to_string();

            if path_str.contains("/target/") || path_str.contains("/.git/") {
                continue;
            }

            let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
            if extensions.contains(&ext) {
                if let Ok(content) = fs::read_to_string(path) {
                    output.push_str(&format!("\n// ═══════════════════════════════════════════\n// FILE: {}\n// ═══════════════════════════════════════════\n{}\n", path_str, content));
                }
            }
        }

        let size = output.len();

        // Try xclip first, then xsel, then wl-copy (Wayland)
        let clipboard_cmd = if Command::new("which")
            .arg("xclip")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
        {
            Some(("xclip", vec!["-selection", "clipboard"]))
        } else if Command::new("which")
            .arg("xsel")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
        {
            Some(("xsel", vec!["--clipboard", "--input"]))
        } else if Command::new("which")
            .arg("wl-copy")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
        {
            Some(("wl-copy", vec![]))
        } else {
            None
        };

        if let Some((cmd, args)) = clipboard_cmd {
            let mut child = Command::new(cmd)
                .args(&args)
                .stdin(Stdio::piped())
                .spawn()?;

            if let Some(stdin) = child.stdin.as_mut() {
                use std::io::Write;
                stdin.write_all(output.as_bytes())?;
            }
            child.wait()?;
        } else {
            return Err(anyhow::anyhow!(
                "No clipboard tool found (xclip, xsel, or wl-copy)"
            ));
        }

        Ok(size)
    }

    // =========================================================================
    // Qwen Code-like Features
    // =========================================================================

    /// Expand @file references in input (e.g., "@src/main.rs" becomes file content)
    /// Also supports @directory/ to include a directory tree (max depth 3)
    /// Returns the expanded input and the list of files that were included
    fn expand_file_references(&self, input: &str) -> (String, Vec<String>) {
        use regex::Regex;
        use std::fs;

        let re = Regex::new(r"@([a-zA-Z0-9_./\-]+(?:\.[a-zA-Z0-9]+)?/?)").unwrap();
        let mut expanded = input.to_string();
        let mut included_files = Vec::new();

        for caps in re.captures_iter(input) {
            let full_match = caps.get(0).unwrap().as_str();
            let file_path = caps.get(1).unwrap().as_str();
            let path = std::path::Path::new(file_path);

            if path.is_dir() {
                // Directory reference: include tree listing + file contents (max depth 3)
                let mut dir_content = format!("Directory tree for {}:\n```\n", file_path);
                let mut file_count = 0;
                for entry in walkdir::WalkDir::new(file_path)
                    .max_depth(3)
                    .into_iter()
                    .filter_map(|e| e.ok())
                {
                    let entry_path = entry.path();
                    let display = entry_path.display().to_string();
                    if display.contains("/target/")
                        || display.contains("/.git/")
                        || display.contains("/node_modules/")
                    {
                        continue;
                    }
                    if entry.file_type().is_file() {
                        dir_content.push_str(&format!("  {}\n", display));
                        file_count += 1;
                    }
                }
                dir_content.push_str("```\n");
                expanded = expanded.replacen(full_match, &dir_content, 1);
                included_files.push(format!(
                    "{}/ ({} files)",
                    file_path.trim_end_matches('/'),
                    file_count
                ));
            } else if let Ok(content) = fs::read_to_string(file_path) {
                let file_block = format!(
                    "\n```{} ({})\n{}\n```\n",
                    file_path,
                    Self::format_file_size(content.len()),
                    content.trim()
                );
                expanded = expanded.replacen(full_match, &file_block, 1);
                included_files.push(file_path.to_string());
            }
        }

        (expanded, included_files)
    }

    /// Format file size for display
    fn format_file_size(bytes: usize) -> String {
        if bytes >= 1024 * 1024 {
            format!("{:.1}MB", bytes as f64 / (1024.0 * 1024.0))
        } else if bytes >= 1024 {
            format!("{:.1}KB", bytes as f64 / 1024.0)
        } else {
            format!("{}B", bytes)
        }
    }

    /// Show detailed session statistics (Qwen Code /stats style)
    fn show_session_stats(&self) {
        let tokens = self.memory.total_tokens();
        let window = self.memory.context_window();
        let used_pct = (tokens as f64 / window as f64 * 100.0).min(100.0);
        let messages = self.messages.len();
        let user_msgs = self.messages.iter().filter(|m| m.role == "user").count();
        let assistant_msgs = self
            .messages
            .iter()
            .filter(|m| m.role == "assistant")
            .count();
        let tool_calls = self
            .messages
            .iter()
            .filter(|m| m.role == "assistant" && m.content.contains("<tool>"))
            .count();

        // Colors - respect --no-color and NO_COLOR env
        let colors_enabled = colored::control::SHOULD_COLORIZE.should_colorize();
        let (rust, patina, sand, worn, reset, bold) = if colors_enabled {
            (
                "\x1b[38;5;130m",
                "\x1b[38;5;66m",
                "\x1b[38;5;180m",
                "\x1b[38;5;245m",
                "\x1b[0m",
                "\x1b[1m",
            )
        } else {
            ("", "", "", "", "", "")
        };

        // Elapsed time since session start (approximation based on messages)
        let session_indicator = if messages > 50 {
            "EXTENDED"
        } else if messages > 20 {
            "ACTIVE"
        } else if messages > 5 {
            "WARM"
        } else {
            "NEW"
        };

        println!();
        println!(
            "  {}┌─────────────────────── {} SESSION STATS {} ───────────────────────┐{}",
            patina, rust, patina, reset
        );
        println!(
            "  {}│{}                                                                    {}│{}",
            patina, reset, patina, reset
        );
        println!(
            "  {}│{}  {bold}{}◈ CONTEXT{}{:<48}    {}│{}",
            patina, reset, rust, reset, "", patina, reset
        );
        println!(
            "  {}│{}     Tokens Used     {:>8} / {:<8}  ({:.1}%)                  {}│{}",
            patina, reset, tokens, window, used_pct, patina, reset
        );
        println!(
            "  {}│{}     Messages        {:>8}  (user: {}, assistant: {})        {}│{}",
            patina, reset, messages, user_msgs, assistant_msgs, patina, reset
        );
        println!(
            "  {}│{}     Tool Calls      {:>8}                                    {}│{}",
            patina, reset, tool_calls, patina, reset
        );
        println!(
            "  {}│{}                                                                    {}│{}",
            patina, reset, patina, reset
        );
        println!(
            "  {}│{}  {bold}{}⊡ MEMORY{}{:<49}    {}│{}",
            patina, reset, sand, reset, "", patina, reset
        );
        println!(
            "  {}│{}     Entries         {:>8}                                    {}│{}",
            patina,
            reset,
            self.memory.len(),
            patina,
            reset
        );
        println!(
            "  {}│{}     Files Loaded    {:>8}                                    {}│{}",
            patina,
            reset,
            self.context_files.len(),
            patina,
            reset
        );
        println!(
            "  {}│{}     Session         {:>8}                                    {}│{}",
            patina, reset, session_indicator, patina, reset
        );
        println!(
            "  {}│{}                                                                    {}│{}",
            patina, reset, patina, reset
        );
        println!(
            "  {}│{}  {bold}{}≋ MODE{}{:<50}    {}│{}",
            patina, reset, worn, reset, "", patina, reset
        );
        let mode_str = match self.execution_mode() {
            crate::config::ExecutionMode::Normal => "NORMAL - Confirm all tools",
            crate::config::ExecutionMode::AutoEdit => "AUTO-EDIT - Auto-approve file ops",
            crate::config::ExecutionMode::Yolo => "YOLO - Execute without confirmation",
            crate::config::ExecutionMode::Daemon => "DAEMON - Permanent auto-execute",
        };
        println!(
            "  {}│{}     {}                                            {}│{}",
            patina, reset, mode_str, patina, reset
        );
        println!(
            "  {}│{}                                                                    {}│{}",
            patina, reset, patina, reset
        );
        println!(
            "  {}└────────────────────────────────────────────────────────────────────┘{}",
            patina, reset
        );
        println!();
    }

    /// Compress context to reduce token usage
    async fn compress_context(&mut self) -> Result<usize> {
        let before = self.compressor.estimate_tokens(&self.messages);

        if !self.compressor.should_compress(&self.messages) {
            println!(
                "{} Context is within limits, no compression needed",
                "ℹ️".bright_cyan()
            );
            return Ok(0);
        }

        println!("{} Compressing context...", "🗜️".bright_cyan());

        self.messages = self
            .compressor
            .compress(&self.client, &self.messages)
            .await?;

        let after = self.compressor.estimate_tokens(&self.messages);
        let saved = before.saturating_sub(after);
        let pct = if before > 0 {
            saved as f64 / before as f64 * 100.0
        } else {
            0.0
        };

        println!(
            "{} Compressed: {} → {} tokens ({:.1}% reduction)",
            "✓".bright_green(),
            before.to_string().bright_yellow(),
            after.to_string().bright_green(),
            pct
        );

        Ok(saved)
    }

    pub async fn run_task(&mut self, task: &str) -> Result<()> {
        println!("{}", "🦊 Selfware starting task...".bright_cyan());
        println!("Task: {}", task.bright_white());

        // Initialize checkpoint if not resuming
        if self.current_checkpoint.is_none() {
            let task_id = uuid::Uuid::new_v4().to_string();
            self.current_checkpoint = Some(TaskCheckpoint::new(task_id, task.to_string()));
        }

        let msg = Message::user(task);
        self.memory.add_message(&msg);
        self.messages.push(msg);

        let mut iteration = 0;
        #[cfg(feature = "resilience")]
        let mut recovery_attempts = 0u32;
        let task_description = task.to_string();

        // Initialize multi-phase progress tracker
        let mut progress = output::TaskProgress::new(&["Planning", "Executing"]);
        progress.start_phase();

        while let Some(state) = self.loop_control.next_state() {
            if self.is_cancelled() {
                println!("{}", "\n⚡ Interrupted".bright_yellow());
                self.messages
                    .push(Message::user("[Task interrupted by user]"));
                return Ok(());
            }

            match state {
                AgentState::Planning => {
                    let _span = enter_agent_step("Planning", 0);
                    record_state_transition("Start", "Planning");
                    output::phase_transition("Start", "Planning");

                    // Set cognitive state to Plan phase
                    self.cognitive_state.set_phase(CyclePhase::Plan);

                    // Plan returns true if the response contains tool calls
                    let has_tool_calls = self.plan().await?;

                    // Transition to Do phase
                    record_state_transition("Planning", "Executing");
                    output::phase_transition("Planning", "Executing");
                    progress.complete_phase(); // Complete planning phase
                    self.cognitive_state.set_phase(CyclePhase::Do);
                    self.loop_control
                        .set_state(AgentState::Executing { step: 0 });

                    // If planning response contained tool calls, execute them now
                    if has_tool_calls {
                        output::step_start(1, "Executing");
                        match self.execute_pending_tool_calls(&task_description).await {
                            Ok(completed) => {
                                if completed {
                                    record_state_transition("Executing", "Completed");
                                    output::task_completed();
                                    if let Err(e) = self.complete_checkpoint() {
                                        warn!("Failed to save completed checkpoint: {}", e);
                                    }
                                    return Ok(());
                                }
                                #[cfg(feature = "resilience")]
                                {
                                    recovery_attempts = 0;
                                }
                                self.loop_control.increment_step();
                                self.cognitive_state.set_phase(CyclePhase::Reflect);
                                self.cognitive_state.working_memory.complete_step(1, None);
                                self.cognitive_state.set_phase(CyclePhase::Do);
                            }
                            Err(e) => {
                                warn!("Initial execution failed: {}", e);

                                // Check for confirmation error - these are fatal in non-interactive mode
                                if is_confirmation_error(&e) {
                                    record_state_transition("Planning", "Failed");
                                    if let Some(ref mut checkpoint) = self.current_checkpoint {
                                        checkpoint.log_error(0, e.to_string(), false);
                                    }
                                    self.loop_control.set_state(AgentState::Failed {
                                        reason: e.to_string(),
                                    });
                                    continue;
                                }

                                self.cognitive_state
                                    .working_memory
                                    .fail_step(1, &e.to_string());
                                if let Some(ref mut checkpoint) = self.current_checkpoint {
                                    checkpoint.log_error(0, e.to_string(), true);
                                }
                                self.loop_control.set_state(AgentState::ErrorRecovery {
                                    error: e.to_string(),
                                });
                            }
                        }
                    }

                    // Save checkpoint after planning
                    if let Err(e) = self.save_checkpoint(&task_description) {
                        warn!("Failed to save checkpoint: {}", e);
                    }
                }
                AgentState::Executing { step } => {
                    let _span = enter_agent_step("Executing", step);
                    output::step_start(step + 1, "Executing");
                    // Update progress based on step
                    let step_progress = ((step + 1) as f64 * 0.1).min(0.9);
                    progress.update_progress(step_progress);
                    match self.execute_step_with_logging(&task_description).await {
                        Ok(completed) => {
                            #[cfg(feature = "resilience")]
                            {
                                recovery_attempts = 0;
                                self.reset_self_healing_retry();
                            }
                            if completed {
                                record_state_transition("Executing", "Completed");
                                progress.complete_phase();
                                output::task_completed();
                                if let Err(e) = self.complete_checkpoint() {
                                    warn!("Failed to save completed checkpoint: {}", e);
                                }
                                return Ok(());
                            }
                            self.loop_control.increment_step();

                            // Reflect phase - update cognitive state
                            self.cognitive_state.set_phase(CyclePhase::Reflect);
                            self.cognitive_state
                                .working_memory
                                .complete_step(step + 1, None);
                            self.cognitive_state.set_phase(CyclePhase::Do);

                            // Save checkpoint after each step
                            if let Err(e) = self.save_checkpoint(&task_description) {
                                warn!("Failed to save checkpoint: {}", e);
                            }
                        }
                        Err(e) => {
                            warn!("Step failed: {}", e);

                            // Check for confirmation error - these are fatal in non-interactive mode
                            if is_confirmation_error(&e) {
                                record_state_transition("Executing", "Failed");
                                if let Some(ref mut checkpoint) = self.current_checkpoint {
                                    checkpoint.log_error(step, e.to_string(), false);
                                }
                                self.loop_control.set_state(AgentState::Failed {
                                    reason: e.to_string(),
                                });
                                continue;
                            }

                            record_state_transition("Executing", "ErrorRecovery");

                            // Record failure in cognitive state
                            self.cognitive_state
                                .working_memory
                                .fail_step(step + 1, &e.to_string());
                            self.cognitive_state
                                .episodic_memory
                                .what_failed("execution", &e.to_string());

                            // Log error in checkpoint
                            if let Some(ref mut checkpoint) = self.current_checkpoint {
                                checkpoint.log_error(step, e.to_string(), true);
                            }
                            self.loop_control.set_state(AgentState::ErrorRecovery {
                                error: e.to_string(),
                            });
                        }
                    }
                }
                AgentState::ErrorRecovery { error } => {
                    let _span = enter_agent_step("ErrorRecovery", self.loop_control.current_step());

                    println!("{} {}", "⚠️ Recovering from error:".bright_red(), error);

                    #[cfg(feature = "resilience")]
                    let mut recovered = false;
                    #[cfg(not(feature = "resilience"))]
                    let recovered = false;
                    #[cfg(feature = "resilience")]
                    {
                        if recovery_attempts < self.config.continuous_work.max_recovery_attempts {
                            recovery_attempts += 1;
                            recovered = self.try_self_healing_recovery(&error, "run_task");
                        } else {
                            warn!(
                                "Auto-recovery attempts exhausted ({})",
                                self.config.continuous_work.max_recovery_attempts
                            );
                        }
                    }

                    if recovered {
                        record_state_transition("ErrorRecovery", "Executing");
                        self.loop_control.set_state(AgentState::Executing {
                            step: self.loop_control.current_step(),
                        });
                        continue;
                    }

                    // Add cognitive context about the error
                    let cognitive_summary = self.cognitive_state.summary();
                    self.messages.push(Message::user(format!(
                        "The previous action failed with error: {}. Please try a different approach.\n\n{}",
                        error, cognitive_summary
                    )));

                    record_state_transition("ErrorRecovery", "Executing");
                    self.loop_control.set_state(AgentState::Executing {
                        step: self.loop_control.current_step(),
                    });
                }
                AgentState::Completed => {
                    record_state_transition("Executing", "Completed");
                    progress.complete_phase();
                    output::task_completed();
                    if let Err(e) = self.complete_checkpoint() {
                        warn!("Failed to save completed checkpoint: {}", e);
                    }
                    return Ok(());
                }
                AgentState::Failed { reason } => {
                    record_state_transition("Executing", "Failed");
                    progress.fail_phase();
                    println!("{} {}", "❌ Task failed:".bright_red(), reason);
                    if let Err(e) = self.fail_checkpoint(&reason) {
                        warn!("Failed to save failed checkpoint: {}", e);
                    }
                    anyhow::bail!("Agent failed: {}", reason);
                }
            }

            iteration += 1;
            if iteration > self.config.agent.max_iterations {
                progress.fail_phase();
                if let Err(e) = self.fail_checkpoint("Max iterations reached") {
                    warn!("Failed to save failed checkpoint: {}", e);
                }
                anyhow::bail!("Max iterations reached");
            }
        }

        Ok(())
    }

    async fn run_swarm_task(&mut self, task: &str) -> Result<()> {
        use crate::orchestration::swarm::create_dev_swarm;

        let swarm = create_dev_swarm();
        let mut agents = swarm.list_agents();
        agents.sort_by_key(|a| std::cmp::Reverse(a.role.priority()));

        println!(
            "{} Swarm initialized: {} agents",
            "🐝".bright_cyan(),
            agents.len()
        );
        for agent in agents {
            println!(
                "  {} {} ({})",
                "→".bright_black(),
                agent.name.bright_white(),
                agent.role.name().dimmed()
            );
        }

        let prompt = format!(
            "You are coordinating a dev team: Architect, Coder, Tester, Reviewer.\n\
             Decompose and execute this task. Verify with cargo_check after changes.\n\
             Task: {}",
            task
        );
        self.run_task(&prompt).await
    }

    pub async fn analyze(&mut self, path: &str) -> Result<()> {
        let task = Planner::analyze_prompt(path);
        self.run_task(&task).await
    }

    /// Review code in a specific file
    pub async fn review(&mut self, file_path: &str) -> Result<()> {
        // Read the file first
        let content = std::fs::read_to_string(file_path)
            .with_context(|| format!("Failed to read file: {}", file_path))?;

        let task = Planner::review_prompt(file_path, &content);
        self.run_task(&task).await
    }

    /// Get memory statistics
    pub fn memory_stats(&self) -> (usize, usize, bool) {
        (
            self.memory.len(),
            self.memory.total_tokens(),
            self.memory.is_near_limit(),
        )
    }

    /// List all saved tasks
    pub fn list_tasks() -> Result<Vec<crate::checkpoint::TaskSummary>> {
        let manager =
            CheckpointManager::default_path().context("Failed to initialize checkpoint manager")?;
        manager.list_tasks()
    }

    /// Get status of a specific task
    pub fn task_status(task_id: &str) -> Result<crate::checkpoint::TaskCheckpoint> {
        let manager =
            CheckpointManager::default_path().context("Failed to initialize checkpoint manager")?;
        manager.load(task_id)
    }

    /// Delete a saved task
    pub fn delete_task(task_id: &str) -> Result<()> {
        let manager =
            CheckpointManager::default_path().context("Failed to initialize checkpoint manager")?;
        manager.delete(task_id)
    }

    /// Enhance cargo check/clippy errors with analyzer suggestions
    fn enhance_cargo_errors(&self, result_str: &str) -> String {
        // Try to parse the result and extract errors
        if let Ok(result) = serde_json::from_str::<Value>(result_str) {
            if let Some(errors) = result.get("errors").and_then(|e| e.as_array()) {
                let raw_errors: Vec<_> = errors
                    .iter()
                    .filter_map(|e| {
                        let code = e.get("code").and_then(|c| c.as_str());
                        let message = e.get("message").and_then(|m| m.as_str())?;
                        let file = e.get("file").and_then(|f| f.as_str()).unwrap_or("unknown");
                        let line = e.get("line").and_then(|l| l.as_u64()).map(|l| l as u32);
                        let column = e.get("column").and_then(|c| c.as_u64()).map(|c| c as u32);
                        Some((code, message, file, line, column))
                    })
                    .collect();

                if !raw_errors.is_empty() {
                    let analyzed = self.error_analyzer.analyze_batch(&raw_errors);
                    let summary = self.error_analyzer.summary(&analyzed);

                    info!(
                        "Enhanced {} errors with analyzer suggestions",
                        analyzed.len()
                    );

                    return format!(
                        "{}\n\n<error_analysis>\n{}\n</error_analysis>",
                        result_str, summary
                    );
                }
            }
        }
        result_str.to_string()
    }

    /// Continue execution from current state (for resuming tasks)
    pub async fn continue_execution(&mut self) -> Result<()> {
        let task_description = self
            .current_checkpoint
            .as_ref()
            .map(|c| c.task_description.clone())
            .unwrap_or_default();

        let mut iteration = 0;
        #[cfg(feature = "resilience")]
        let mut recovery_attempts = 0u32;

        while let Some(state) = self.loop_control.next_state() {
            match state {
                AgentState::Planning => {
                    let _span = enter_agent_step("Planning", 0);
                    record_state_transition("Resume", "Planning");
                    println!("{}", "📋 Planning...".bright_yellow());
                    self.cognitive_state.set_phase(CyclePhase::Plan);

                    self.plan().await?;
                    self.loop_control
                        .set_state(AgentState::Executing { step: 0 });
                    self.cognitive_state.set_phase(CyclePhase::Do);

                    if let Err(e) = self.save_checkpoint(&task_description) {
                        warn!("Failed to save checkpoint: {}", e);
                    }
                }
                AgentState::Executing { step } => {
                    let _span = enter_agent_step("Executing", step);
                    println!(
                        "{} Executing...",
                        format!("📝 Step {}", step + 1).bright_blue()
                    );
                    match self.execute_step_with_logging(&task_description).await {
                        Ok(completed) => {
                            #[cfg(feature = "resilience")]
                            {
                                recovery_attempts = 0;
                                self.reset_self_healing_retry();
                            }
                            if completed {
                                record_state_transition("Executing", "Completed");
                                output::task_completed();
                                if let Err(e) = self.complete_checkpoint() {
                                    warn!("Failed to save completed checkpoint: {}", e);
                                }
                                return Ok(());
                            }
                            self.loop_control.increment_step();

                            // Reflect and continue
                            self.cognitive_state.set_phase(CyclePhase::Reflect);
                            self.cognitive_state
                                .working_memory
                                .complete_step(step + 1, None);
                            self.cognitive_state.set_phase(CyclePhase::Do);

                            if let Err(e) = self.save_checkpoint(&task_description) {
                                warn!("Failed to save checkpoint: {}", e);
                            }
                        }
                        Err(e) => {
                            warn!("Step failed: {}", e);

                            // Check for confirmation error - these are fatal in non-interactive mode
                            if is_confirmation_error(&e) {
                                record_state_transition("Executing", "Failed");
                                if let Some(ref mut checkpoint) = self.current_checkpoint {
                                    checkpoint.log_error(step, e.to_string(), false);
                                }
                                self.loop_control.set_state(AgentState::Failed {
                                    reason: e.to_string(),
                                });
                                continue;
                            }

                            record_state_transition("Executing", "ErrorRecovery");
                            self.cognitive_state
                                .working_memory
                                .fail_step(step + 1, &e.to_string());

                            if let Some(ref mut checkpoint) = self.current_checkpoint {
                                checkpoint.log_error(step, e.to_string(), true);
                            }
                            self.loop_control.set_state(AgentState::ErrorRecovery {
                                error: e.to_string(),
                            });
                        }
                    }
                }
                AgentState::ErrorRecovery { error } => {
                    let _span = enter_agent_step("ErrorRecovery", self.loop_control.current_step());

                    println!("{} {}", "⚠️ Recovering from error:".bright_red(), error);

                    #[cfg(feature = "resilience")]
                    let mut recovered = false;
                    #[cfg(not(feature = "resilience"))]
                    let recovered = false;
                    #[cfg(feature = "resilience")]
                    {
                        if recovery_attempts < self.config.continuous_work.max_recovery_attempts {
                            recovery_attempts += 1;
                            recovered =
                                self.try_self_healing_recovery(&error, "continue_execution");
                        } else {
                            warn!(
                                "Auto-recovery attempts exhausted ({})",
                                self.config.continuous_work.max_recovery_attempts
                            );
                        }
                    }

                    if recovered {
                        record_state_transition("ErrorRecovery", "Executing");
                        self.loop_control.set_state(AgentState::Executing {
                            step: self.loop_control.current_step(),
                        });
                        continue;
                    }

                    let cognitive_summary = self.cognitive_state.summary();
                    self.messages.push(Message::user(format!(
                        "The previous action failed with error: {}. Please try a different approach.\n\n{}",
                        error, cognitive_summary
                    )));

                    record_state_transition("ErrorRecovery", "Executing");
                    self.loop_control.set_state(AgentState::Executing {
                        step: self.loop_control.current_step(),
                    });
                }
                AgentState::Completed => {
                    record_state_transition("Executing", "Completed");
                    output::task_completed();
                    if let Err(e) = self.complete_checkpoint() {
                        warn!("Failed to save completed checkpoint: {}", e);
                    }
                    return Ok(());
                }
                AgentState::Failed { reason } => {
                    record_state_transition("Executing", "Failed");
                    println!("{} {}", "❌ Task failed:".bright_red(), reason);
                    if let Err(e) = self.fail_checkpoint(&reason) {
                        warn!("Failed to save failed checkpoint: {}", e);
                    }
                    anyhow::bail!("Agent failed: {}", reason);
                }
            }

            iteration += 1;
            if iteration > self.config.agent.max_iterations {
                if let Err(e) = self.fail_checkpoint("Max iterations reached") {
                    warn!("Failed to save failed checkpoint: {}", e);
                }
                anyhow::bail!("Max iterations reached");
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::types::{ToolCall, ToolFunction};
    use crate::config::{Config, ExecutionMode};
    use crate::tool_parser::parse_tool_calls;
    use loop_control::{AgentLoop, AgentState};

    // =========================================================================
    // Test 1: Agent State Transitions
    // =========================================================================

    #[test]
    fn test_agent_state_transitions_idle_to_planning() {
        // AgentLoop starts in Planning state (not Idle, as there's no Idle state)
        let mut loop_ctrl = AgentLoop::new(100);

        // First state should be Planning
        let state = loop_ctrl.next_state();
        assert!(matches!(state, Some(AgentState::Planning)));

        // Transition to Executing
        loop_ctrl.set_state(AgentState::Executing { step: 0 });
        let state = loop_ctrl.next_state();
        assert!(matches!(state, Some(AgentState::Executing { step: 0 })));
    }

    #[test]
    fn test_agent_state_transitions_planning_to_executing() {
        let mut loop_ctrl = AgentLoop::new(100);

        // Start in Planning
        let _ = loop_ctrl.next_state();
        assert!(matches!(loop_ctrl.next_state(), Some(AgentState::Planning)));

        // Transition to Executing with step 0
        loop_ctrl.set_state(AgentState::Executing { step: 0 });
        let state = loop_ctrl.next_state();
        match state {
            Some(AgentState::Executing { step }) => assert_eq!(step, 0),
            _ => panic!("Expected Executing state with step 0"),
        }
    }

    #[test]
    fn test_agent_state_transitions_executing_to_completed() {
        let mut loop_ctrl = AgentLoop::new(100);

        // Start execution
        loop_ctrl.set_state(AgentState::Executing { step: 0 });
        let _ = loop_ctrl.next_state();

        // Simulate task completion
        loop_ctrl.set_state(AgentState::Completed);
        let state = loop_ctrl.next_state();
        assert!(matches!(state, Some(AgentState::Completed)));
    }

    #[test]
    fn test_agent_state_transitions_executing_to_error_recovery() {
        let mut loop_ctrl = AgentLoop::new(100);

        // Start execution
        loop_ctrl.set_state(AgentState::Executing { step: 0 });
        let _ = loop_ctrl.next_state();

        // Simulate error
        loop_ctrl.set_state(AgentState::ErrorRecovery {
            error: "Tool execution failed".to_string(),
        });
        let state = loop_ctrl.next_state();
        match state {
            Some(AgentState::ErrorRecovery { error }) => {
                assert_eq!(error, "Tool execution failed");
            }
            _ => panic!("Expected ErrorRecovery state"),
        }
    }

    #[test]
    fn test_agent_state_full_lifecycle() {
        let mut loop_ctrl = AgentLoop::new(100);

        // Planning -> Executing -> Error -> Recovery -> Executing -> Completed
        assert!(matches!(loop_ctrl.next_state(), Some(AgentState::Planning)));

        loop_ctrl.set_state(AgentState::Executing { step: 0 });
        assert!(matches!(
            loop_ctrl.next_state(),
            Some(AgentState::Executing { .. })
        ));

        loop_ctrl.set_state(AgentState::ErrorRecovery {
            error: "test".to_string(),
        });
        assert!(matches!(
            loop_ctrl.next_state(),
            Some(AgentState::ErrorRecovery { .. })
        ));

        loop_ctrl.set_state(AgentState::Executing { step: 1 });
        assert!(matches!(
            loop_ctrl.next_state(),
            Some(AgentState::Executing { step: 1 })
        ));

        loop_ctrl.set_state(AgentState::Completed);
        assert!(matches!(
            loop_ctrl.next_state(),
            Some(AgentState::Completed)
        ));
    }

    // =========================================================================
    // Test 2: Tool Call Handling with Mock Data
    // =========================================================================

    fn create_mock_tool_call(name: &str, args: &str) -> ToolCall {
        ToolCall {
            id: format!("call_{}", uuid::Uuid::new_v4()),
            call_type: "function".to_string(),
            function: ToolFunction {
                name: name.to_string(),
                arguments: args.to_string(),
            },
        }
    }

    #[test]
    fn test_tool_call_parsing_xml_format() {
        let content = r#"
        Let me read that file for you.

        <tool>
        <name>file_read</name>
        <arguments>{"path": "./src/main.rs"}</arguments>
        </tool>
        "#;

        let result = parse_tool_calls(content);
        assert_eq!(result.tool_calls.len(), 1);
        assert_eq!(result.tool_calls[0].tool_name, "file_read");

        let args = &result.tool_calls[0].arguments;
        assert_eq!(args["path"], "./src/main.rs");
    }

    #[test]
    fn test_tool_call_parsing_multiple_tools() {
        let content = r#"
        I'll check the git status and read a file.

        <tool>
        <name>git_status</name>
        <arguments>{}</arguments>
        </tool>

        <tool>
        <name>file_read</name>
        <arguments>{"path": "Cargo.toml"}</arguments>
        </tool>
        "#;

        let result = parse_tool_calls(content);
        assert_eq!(result.tool_calls.len(), 2);
        assert_eq!(result.tool_calls[0].tool_name, "git_status");
        assert_eq!(result.tool_calls[1].tool_name, "file_read");
    }

    #[test]
    fn test_tool_call_with_complex_arguments() {
        let content = r#"
        <tool>
        <name>file_edit</name>
        <arguments>{
            "path": "./src/lib.rs",
            "old_str": "fn old_function() {\n    println!(\"old\");\n}",
            "new_str": "fn new_function() {\n    println!(\"new\");\n}"
        }</arguments>
        </tool>
        "#;

        let result = parse_tool_calls(content);
        assert_eq!(result.tool_calls.len(), 1);
        assert_eq!(result.tool_calls[0].tool_name, "file_edit");

        let args = &result.tool_calls[0].arguments;
        assert!(args["old_str"].as_str().unwrap().contains("old_function"));
        assert!(args["new_str"].as_str().unwrap().contains("new_function"));
    }

    #[test]
    fn test_tool_call_no_tools_in_content() {
        let content = "This is just a regular response without any tool calls.";

        let result = parse_tool_calls(content);
        assert!(result.tool_calls.is_empty());
        assert!(!result.text_content.is_empty());
    }

    #[test]
    fn test_mock_tool_call_creation() {
        let call = create_mock_tool_call("shell_exec", r#"{"command": "ls -la"}"#);
        assert_eq!(call.function.name, "shell_exec");
        assert!(call.function.arguments.contains("ls -la"));
        assert_eq!(call.call_type, "function");
        assert!(call.id.starts_with("call_"));
    }

    // =========================================================================
    // Test 3: Error Recovery Scenarios
    // =========================================================================

    #[test]
    fn test_error_recovery_state_preserves_error_message() {
        let mut loop_ctrl = AgentLoop::new(100);

        let error_message = "Connection timeout while calling external API";
        loop_ctrl.set_state(AgentState::ErrorRecovery {
            error: error_message.to_string(),
        });

        let state = loop_ctrl.next_state();
        match state {
            Some(AgentState::ErrorRecovery { error }) => {
                assert_eq!(error, error_message);
            }
            _ => panic!("Expected ErrorRecovery state"),
        }
    }

    #[test]
    fn test_error_recovery_transitions_back_to_executing() {
        let mut loop_ctrl = AgentLoop::new(100);

        // Enter error recovery
        loop_ctrl.set_state(AgentState::ErrorRecovery {
            error: "some error".to_string(),
        });
        let _ = loop_ctrl.next_state();

        // Transition back to executing after recovery
        let current_step = loop_ctrl.current_step();
        loop_ctrl.set_state(AgentState::Executing { step: current_step });
        let state = loop_ctrl.next_state();
        assert!(matches!(state, Some(AgentState::Executing { .. })));
    }

    #[test]
    fn test_error_recovery_can_transition_to_failed() {
        let mut loop_ctrl = AgentLoop::new(100);

        // Enter error recovery
        loop_ctrl.set_state(AgentState::ErrorRecovery {
            error: "unrecoverable error".to_string(),
        });
        let _ = loop_ctrl.next_state();

        // If recovery fails, transition to Failed
        loop_ctrl.set_state(AgentState::Failed {
            reason: "Max retries exceeded".to_string(),
        });
        let state = loop_ctrl.next_state();
        match state {
            Some(AgentState::Failed { reason }) => {
                assert_eq!(reason, "Max retries exceeded");
            }
            _ => panic!("Expected Failed state"),
        }
    }

    #[test]
    fn test_confirmation_error_detection() {
        let error = AgentError::ConfirmationRequired {
            tool_name: "shell_exec".to_string(),
        };
        let anyhow_error: anyhow::Error = error.into();

        assert!(is_confirmation_error(&anyhow_error));
    }

    #[test]
    fn test_non_confirmation_error_detection() {
        let error = anyhow::anyhow!("Some other error");
        assert!(!is_confirmation_error(&error));
    }

    // =========================================================================
    // Test 4: Context Compression Triggers
    // =========================================================================

    #[test]
    fn test_context_compressor_threshold_calculation() {
        let compressor = ContextCompressor::new(100000);
        // Threshold is 85% of budget
        assert!(!compressor.should_compress(&[]));

        // Create messages that exceed threshold
        let mut large_messages = vec![Message::system("System prompt")];
        for _ in 0..100 {
            large_messages.push(Message::user("x".repeat(1000)));
        }

        // With 100 messages of ~1000 chars each, this should trigger compression
        let compressor_small = ContextCompressor::new(10000);
        assert!(compressor_small.should_compress(&large_messages));
    }

    #[test]
    fn test_context_compressor_estimate_tokens() {
        let compressor = ContextCompressor::new(100000);

        let messages = vec![
            Message::system("You are a helpful assistant"),
            Message::user("Hello, how are you?"),
            Message::assistant("I'm doing well, thank you!"),
        ];

        let estimate = compressor.estimate_tokens(&messages);
        // Should have reasonable estimate (base cost + content)
        assert!(estimate > 150); // 3 messages * ~50 base minimum
        assert!(estimate < 500); // Shouldn't be too high for short messages
    }

    #[test]
    fn test_context_compressor_code_content_factor() {
        let compressor = ContextCompressor::new(100000);

        // Code content (with braces) uses factor 3
        let code_msg = vec![Message::user("fn main() { println!(\"hello\"); }")];

        // Plain text uses factor 4
        let text_msg = vec![Message::user("This is plain text content")];

        let code_estimate = compressor.estimate_tokens(&code_msg);
        let text_estimate = compressor.estimate_tokens(&text_msg);

        // Both should have reasonable estimates
        assert!(code_estimate > 50);
        assert!(text_estimate > 50);
    }

    #[test]
    fn test_hard_compress_preserves_structure() {
        let compressor = ContextCompressor::new(100000);

        let messages = vec![
            Message::system("system prompt"),
            Message::user("question 1"),
            Message::assistant("answer 1"),
            Message::user("question 2"),
            Message::assistant("answer 2"),
            Message::user("recent question"),
        ];

        let compressed = compressor.hard_compress(&messages);

        // Should preserve system message
        assert_eq!(compressed[0].role, "system");

        // Should end with user message
        let last = compressed.last().unwrap();
        assert_eq!(last.role, "user");
    }

    // =========================================================================
    // Test 5: Execution Mode and Tool Confirmation
    // =========================================================================

    #[test]
    fn test_execution_mode_normal_needs_confirmation() {
        let config = Config {
            execution_mode: ExecutionMode::Normal,
            ..Default::default()
        };

        // In normal mode, most tools need confirmation
        // Safe tools (read-only) don't need confirmation
        let safe_tools = [
            "file_read",
            "directory_tree",
            "glob_find",
            "grep_search",
            "git_status",
            "git_diff",
            "git_log",
        ];

        for tool in &safe_tools {
            // Safe tools shouldn't need confirmation even in normal mode
            assert!(
                !needs_confirmation_for_tool(&config, tool),
                "{} should not need confirmation",
                tool
            );
        }

        // Dangerous tools need confirmation in normal mode
        let dangerous_tools = ["shell_exec", "file_write", "git_commit"];
        for tool in &dangerous_tools {
            assert!(
                needs_confirmation_for_tool(&config, tool),
                "{} should need confirmation",
                tool
            );
        }
    }

    #[test]
    fn test_execution_mode_yolo_no_confirmation() {
        let config = Config {
            execution_mode: ExecutionMode::Yolo,
            ..Default::default()
        };

        // In YOLO mode, nothing needs confirmation
        let all_tools = [
            "file_read",
            "file_write",
            "shell_exec",
            "git_commit",
            "cargo_test",
        ];

        for tool in &all_tools {
            assert!(
                !needs_confirmation_for_tool(&config, tool),
                "{} should not need confirmation in YOLO mode",
                tool
            );
        }
    }

    #[test]
    fn test_execution_mode_auto_edit_file_ops() {
        let config = Config {
            execution_mode: ExecutionMode::AutoEdit,
            ..Default::default()
        };

        // Auto-edit mode auto-approves file operations
        assert!(!needs_confirmation_for_tool(&config, "file_write"));
        assert!(!needs_confirmation_for_tool(&config, "file_edit"));

        // But still asks for other operations
        assert!(needs_confirmation_for_tool(&config, "shell_exec"));
        assert!(needs_confirmation_for_tool(&config, "git_commit"));
    }

    #[test]
    fn test_execution_mode_cycle() {
        let mut mode = ExecutionMode::Normal;

        // Normal -> AutoEdit
        mode = cycle_mode(mode);
        assert_eq!(mode, ExecutionMode::AutoEdit);

        // AutoEdit -> Yolo
        mode = cycle_mode(mode);
        assert_eq!(mode, ExecutionMode::Yolo);

        // Yolo -> Normal
        mode = cycle_mode(mode);
        assert_eq!(mode, ExecutionMode::Normal);
    }

    // Helper function to check confirmation without full Agent
    fn needs_confirmation_for_tool(config: &Config, tool_name: &str) -> bool {
        let safe_tools = [
            "file_read",
            "directory_tree",
            "glob_find",
            "grep_search",
            "git_status",
            "git_diff",
            "git_log",
            "ripgrep_search",
            "web_search",
        ];

        if safe_tools.contains(&tool_name) {
            return false;
        }

        match config.execution_mode {
            ExecutionMode::Yolo | ExecutionMode::Daemon => false,
            ExecutionMode::AutoEdit => !matches!(
                tool_name,
                "file_write" | "file_edit" | "file_create" | "directory_tree" | "glob_find"
            ),
            ExecutionMode::Normal => !safe_tools.contains(&tool_name),
        }
    }

    // Helper function to cycle execution mode
    fn cycle_mode(mode: ExecutionMode) -> ExecutionMode {
        match mode {
            ExecutionMode::Normal => ExecutionMode::AutoEdit,
            ExecutionMode::AutoEdit => ExecutionMode::Yolo,
            ExecutionMode::Yolo => ExecutionMode::Normal,
            ExecutionMode::Daemon => ExecutionMode::Normal,
        }
    }

    // =========================================================================
    // Additional Edge Case Tests
    // =========================================================================

    #[test]
    fn test_agent_error_display() {
        let error = AgentError::ConfirmationRequired {
            tool_name: "dangerous_tool".to_string(),
        };
        let display = format!("{}", error);
        assert!(display.contains("dangerous_tool"));
        assert!(display.contains("requires confirmation"));
    }

    #[test]
    fn test_max_iterations_triggers_failure() {
        let mut loop_ctrl = AgentLoop::new(3);

        // Use up all iterations
        loop_ctrl.next_state(); // 1
        loop_ctrl.next_state(); // 2
        loop_ctrl.next_state(); // 3

        // Next should fail
        let state = loop_ctrl.next_state();
        assert!(matches!(
            state,
            Some(AgentState::Failed { reason }) if reason.contains("Max iterations")
        ));
    }

    #[test]
    fn test_step_increment_updates_state() {
        let mut loop_ctrl = AgentLoop::new(100);

        assert_eq!(loop_ctrl.current_step(), 0);

        loop_ctrl.increment_step();
        assert_eq!(loop_ctrl.current_step(), 1);

        // State should be updated to Executing with new step
        let state = loop_ctrl.next_state();
        match state {
            Some(AgentState::Executing { step }) => assert_eq!(step, 1),
            _ => panic!("Expected Executing state with step 1"),
        }
    }

    #[test]
    fn test_tool_call_with_invalid_json_uses_fallback() {
        let content = r#"
        <tool>
        <name>file_read</name>
        <arguments>this is not valid json</arguments>
        </tool>
        "#;

        let result = parse_tool_calls(content);
        // Parser uses fallback - wraps invalid JSON in {"input": "..."}
        assert_eq!(result.tool_calls.len(), 1);
        assert_eq!(result.tool_calls[0].tool_name, "file_read");
        // The fallback wraps plain text in {"input": "..."}
        assert!(result.tool_calls[0].arguments.get("input").is_some());
    }

    #[test]
    fn test_agent_state_clone() {
        let state = AgentState::Executing { step: 5 };
        let cloned = state.clone();

        match cloned {
            AgentState::Executing { step } => assert_eq!(step, 5),
            _ => panic!("Clone should preserve state type and data"),
        }
    }

    #[test]
    fn test_agent_state_debug() {
        let state = AgentState::ErrorRecovery {
            error: "test error".to_string(),
        };
        let debug_str = format!("{:?}", state);

        assert!(debug_str.contains("ErrorRecovery"));
        assert!(debug_str.contains("test error"));
    }
}
