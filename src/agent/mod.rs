use anyhow::{Context, Result};
use colored::*;
use serde_json::Value;
use std::time::Instant;
use tracing::{debug, info, warn};

use crate::analyzer::ErrorAnalyzer;
use crate::api::types::{Message, ToolCall};
use crate::api::{ApiClient, StreamChunk, ThinkingMode};
use crate::checkpoint::{capture_git_state, CheckpointManager, TaskCheckpoint, TaskStatus};
use crate::cognitive::{CognitiveState, CyclePhase};
use crate::config::Config;
use crate::memory::AgentMemory;
use crate::output;
use crate::safety::SafetyChecker;
use crate::telemetry::{enter_agent_step, record_state_transition};
use crate::tools::ToolRegistry;
use crate::verification::{VerificationConfig, VerificationGate};

pub mod context;
mod execution;
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
    /// Last time a checkpoint was persisted to disk
    last_checkpoint_persisted_at: Instant,
    /// Tool call count at last persisted checkpoint
    last_checkpoint_tool_calls: usize,
    /// Whether at least one checkpoint has been persisted in this session
    checkpoint_persisted_once: bool,
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
            last_checkpoint_persisted_at: Instant::now(),
            last_checkpoint_tool_calls: 0,
            checkpoint_persisted_once: false,
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

    /// Chat with streaming, displaying output as it arrives
    /// Returns (content, reasoning, tool_calls) tuple
    async fn chat_streaming(
        &self,
        messages: Vec<Message>,
        tools: Option<Vec<crate::api::types::ToolDefinition>>,
        thinking: ThinkingMode,
    ) -> Result<(String, Option<String>, Option<Vec<ToolCall>>)> {
        use std::io::{self, Write};

        let stream = self.client.chat_stream(messages, tools, thinking).await?;

        let mut rx = stream.into_channel().await;
        let mut content = String::new();
        let mut reasoning = String::new();
        let mut tool_calls: Vec<ToolCall> = Vec::new();
        let mut in_reasoning = false;

        while let Some(chunk_result) = rx.recv().await {
            let chunk = chunk_result?;

            match chunk {
                StreamChunk::Content(text) => {
                    if in_reasoning {
                        // Finished reasoning, now showing content
                        in_reasoning = false;
                        if !output::is_compact() {
                            println!(); // End reasoning line
                        }
                    }
                    print!("{}", text);
                    io::stdout().flush().ok();
                    content.push_str(&text);
                }
                StreamChunk::Reasoning(text) => {
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
    pub fn execution_mode(&self) -> crate::config::ExecutionMode {
        self.config.execution_mode
    }

    /// Set execution mode
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
    pub fn is_interactive(&self) -> bool {
        use std::io::IsTerminal;
        std::io::stdin().is_terminal()
    }

    // =========================================================================
    // Context Management
    // =========================================================================

    /// Show context statistics with visual progress bar
    fn show_context_stats(&self) {
        let tokens = self.memory.total_tokens();
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
        println!();
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

                    // Add to context files tracking
                    self.context_files.push(path_str.clone());

                    // Add as user message with file content
                    self.messages
                        .push(Message::user(format!("{}{}", file_header, content)));

                    let size: String = if content.len() > 1000 {
                        format!("{}K", content.len() / 1000)
                    } else {
                        format!("{}", content.len())
                    };
                    println!(
                        "  {} {} ({})",
                        "✓".bright_green(),
                        path_str.bright_white(),
                        size.bright_black()
                    );
                    loaded += 1;
                }
            }
        }

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

        // Clear old file messages but keep system and conversation
        self.messages
            .retain(|m| m.role == "system" || m.role == "assistant");

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
    /// Returns the expanded input and the list of files that were included
    fn expand_file_references(&self, input: &str) -> (String, Vec<String>) {
        use regex::Regex;
        use std::fs;

        let re = Regex::new(r"@([a-zA-Z0-9_./\-]+(?:\.[a-zA-Z0-9]+)?)").unwrap();
        let mut expanded = input.to_string();
        let mut included_files = Vec::new();

        for caps in re.captures_iter(input) {
            let full_match = caps.get(0).unwrap().as_str();
            let file_path = caps.get(1).unwrap().as_str();

            if let Ok(content) = fs::read_to_string(file_path) {
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

    /// Resume a task from a checkpoint
    pub async fn resume(config: Config, task_id: &str) -> Result<Self> {
        let checkpoint_manager =
            CheckpointManager::default_path().context("Failed to initialize checkpoint manager")?;

        let checkpoint = checkpoint_manager
            .load(task_id)
            .with_context(|| format!("Failed to load checkpoint for task: {}", task_id))?;

        println!(
            "{} Resuming task: {}",
            "🔄".bright_cyan(),
            checkpoint.task_description.bright_white()
        );
        println!(
            "   Current step: {}, Status: {:?}",
            checkpoint.current_step, checkpoint.status
        );

        let mut agent = Self::new(config).await?;

        // Restore state from checkpoint
        agent.messages = checkpoint.messages.clone();
        agent.loop_control = AgentLoop::new(agent.config.agent.max_iterations);
        agent.loop_control.set_state(AgentState::Executing {
            step: checkpoint.current_step,
        });

        // Set current step by calling increment_step the right number of times
        for _ in 0..checkpoint.current_step {
            agent.loop_control.increment_step();
        }

        let checkpoint_tool_calls = checkpoint.tool_calls.len();
        agent.current_checkpoint = Some(checkpoint);
        agent.checkpoint_manager = Some(checkpoint_manager);
        agent.last_checkpoint_tool_calls = checkpoint_tool_calls;
        agent.last_checkpoint_persisted_at = Instant::now();
        agent.checkpoint_persisted_once = true;

        // Set cognitive state to Do phase since we're resuming execution
        agent.cognitive_state.set_phase(CyclePhase::Do);

        info!("Agent resumed from checkpoint with cognitive state in Do phase");

        Ok(agent)
    }

    /// Convert current state to a checkpoint
    pub fn to_checkpoint(&self, task_id: &str, task_description: &str) -> TaskCheckpoint {
        let mut checkpoint = if let Some(ref existing) = self.current_checkpoint {
            existing.clone()
        } else {
            TaskCheckpoint::new(task_id.to_string(), task_description.to_string())
        };

        checkpoint.set_step(self.loop_control.current_step());
        checkpoint.set_messages(self.messages.clone());
        checkpoint.estimated_tokens = self.memory.total_tokens();

        // Capture git state
        if let Ok(cwd) = std::env::current_dir() {
            checkpoint.git_checkpoint = capture_git_state(cwd.to_string_lossy().as_ref());
        }

        checkpoint
    }

    /// Save current state to checkpoint
    fn save_checkpoint(&mut self, task_description: &str) -> Result<()> {
        if let Some(ref manager) = self.checkpoint_manager {
            if !self.should_persist_checkpoint() {
                debug!("Checkpoint skipped by continuous-work policy");
                return Ok(());
            }

            let task_id = self
                .current_checkpoint
                .as_ref()
                .map(|c| c.task_id.clone())
                .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

            let checkpoint = self.to_checkpoint(&task_id, task_description);
            manager.save(&checkpoint)?;
            self.last_checkpoint_tool_calls = checkpoint.tool_calls.len();
            self.last_checkpoint_persisted_at = Instant::now();
            self.checkpoint_persisted_once = true;
            self.current_checkpoint = Some(checkpoint);
            debug!("Checkpoint saved for task: {}", task_id);
        }
        Ok(())
    }

    fn should_persist_checkpoint(&self) -> bool {
        if !self.config.continuous_work.enabled {
            return true;
        }

        if !self.checkpoint_persisted_once {
            return true;
        }

        let tools_interval = self.config.continuous_work.checkpoint_interval_tools;
        let secs_interval = self.config.continuous_work.checkpoint_interval_secs;

        if tools_interval == 0 && secs_interval == 0 {
            return true;
        }

        let current_tool_calls = self
            .current_checkpoint
            .as_ref()
            .map(|c| c.tool_calls.len())
            .unwrap_or(0);
        let tool_calls_elapsed = current_tool_calls.saturating_sub(self.last_checkpoint_tool_calls);
        let time_elapsed = self.last_checkpoint_persisted_at.elapsed().as_secs();

        let reached_tool_interval = tools_interval > 0 && tool_calls_elapsed >= tools_interval;
        let reached_time_interval = secs_interval > 0 && time_elapsed >= secs_interval;

        reached_tool_interval || reached_time_interval
    }

    /// Mark current task as completed
    fn complete_checkpoint(&mut self) -> Result<()> {
        if let Some(ref mut checkpoint) = self.current_checkpoint {
            checkpoint.set_status(TaskStatus::Completed);
            if let Some(ref manager) = self.checkpoint_manager {
                manager.save(checkpoint)?;
                self.last_checkpoint_tool_calls = checkpoint.tool_calls.len();
                self.last_checkpoint_persisted_at = Instant::now();
                self.checkpoint_persisted_once = true;
            }
        }
        Ok(())
    }

    /// Mark current task as failed
    fn fail_checkpoint(&mut self, reason: &str) -> Result<()> {
        if let Some(ref mut checkpoint) = self.current_checkpoint {
            checkpoint.set_status(TaskStatus::Failed);
            checkpoint.log_error(self.loop_control.current_step(), reason.to_string(), false);
            if let Some(ref manager) = self.checkpoint_manager {
                manager.save(checkpoint)?;
                self.last_checkpoint_tool_calls = checkpoint.tool_calls.len();
                self.last_checkpoint_persisted_at = Instant::now();
                self.checkpoint_persisted_once = true;
            }
        }
        Ok(())
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
        let task_description = task.to_string();

        // Initialize multi-phase progress tracker
        let mut progress = output::TaskProgress::new(&["Planning", "Executing"]);
        progress.start_phase();

        while let Some(state) = self.loop_control.next_state() {
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

    pub async fn interactive(&mut self) -> Result<()> {
        use crate::input::{InputConfig, ReadlineResult, SelfwareEditor};

        // Get tool names for autocomplete
        let tool_names: Vec<String> = self
            .tools
            .list()
            .iter()
            .map(|t| t.name().to_string())
            .collect();

        // Create the editor with autocomplete
        let config = InputConfig {
            tool_names,
            ..Default::default()
        };

        let mut editor = match SelfwareEditor::new(config) {
            Ok(e) => e,
            Err(e) => {
                // Fall back to basic input if reedline fails
                eprintln!("Note: Advanced input unavailable ({}), using basic mode", e);
                return self.interactive_basic().await;
            }
        };

        // Display current mode
        let mode_indicator = match self.execution_mode() {
            crate::config::ExecutionMode::Normal => "[normal]",
            crate::config::ExecutionMode::AutoEdit => "[auto-edit]",
            crate::config::ExecutionMode::Yolo => "[YOLO]",
            crate::config::ExecutionMode::Daemon => "[DAEMON]",
        };

        println!(
            "{} {}",
            "🦊 Selfware Workshop Interactive Mode".bright_cyan(),
            mode_indicator.bright_yellow()
        );
        println!("Type 'exit' to quit, '/help' for commands, '/mode' to cycle modes");

        let mut consecutive_errors = 0;
        const MAX_CONSECUTIVE_ERRORS: u32 = 3;

        loop {
            let input = match editor.read_line() {
                Ok(ReadlineResult::Line(line)) => {
                    consecutive_errors = 0;
                    line
                }
                Ok(ReadlineResult::Interrupt) => {
                    consecutive_errors = 0;
                    println!("\n{}", "Interrupted. Type 'exit' to leave.".bright_yellow());
                    continue;
                }
                Ok(ReadlineResult::Eof) => break,
                Err(e) => {
                    consecutive_errors += 1;
                    if consecutive_errors >= MAX_CONSECUTIVE_ERRORS {
                        eprintln!("Terminal input unavailable, falling back to basic mode...");
                        return self.interactive_basic().await;
                    }
                    eprintln!("Input error: {}", e);
                    continue;
                }
            };

            let input = input.trim();

            if input == "exit" || input == "quit" {
                break;
            }

            if input == "/help" {
                println!();
                println!(
                    "{}",
                    "╭──────────────────────────────────────────────────────╮".bright_cyan()
                );
                println!(
                    "{}",
                    "│                 🦊 SELFWARE COMMANDS                 │".bright_cyan()
                );
                println!(
                    "{}",
                    "├──────────────────────────────────────────────────────┤".bright_cyan()
                );
                println!(
                    "│  {} /help              Show this help               │",
                    "📖".bright_white()
                );
                println!(
                    "│  {} /status            Agent status                 │",
                    "📊".bright_white()
                );
                println!(
                    "│  {} /stats             Detailed session stats       │",
                    "📈".bright_white()
                );
                println!(
                    "│  {} /mode              Cycle execution mode         │",
                    "🔄".bright_white()
                );
                println!(
                    "{}",
                    "├──────────────────────────────────────────────────────┤".bright_cyan()
                );
                println!(
                    "│  {} /ctx               Context window stats         │",
                    "📊".bright_white()
                );
                println!(
                    "│  {} /ctx clear         Clear all context            │",
                    "🗑️ ".bright_white()
                );
                println!(
                    "│  {} /ctx load <ext>    Load files (.rs,.toml)       │",
                    "📂".bright_white()
                );
                println!(
                    "│  {} /ctx reload        Reload loaded files          │",
                    "🔄".bright_white()
                );
                println!(
                    "│  {} /ctx copy          Copy sources to clip         │",
                    "📋".bright_white()
                );
                println!(
                    "│  {} /compress          Compress context             │",
                    "🗜️ ".bright_white()
                );
                println!(
                    "{}",
                    "├─────────────────────────────────────────────────┤".bright_cyan()
                );
                println!(
                    "│  {} /memory           Memory statistics        │",
                    "🧠".bright_white()
                );
                println!(
                    "│  {} /clear            Clear conversation       │",
                    "🗑️ ".bright_white()
                );
                println!(
                    "│  {} /tools             List available tools       │",
                    "🔧".bright_white()
                );
                println!(
                    "{}",
                    "├──────────────────────────────────────────────────────┤".bright_cyan()
                );
                println!(
                    "│  {} /analyze <path>    Analyze codebase             │",
                    "🔍".bright_white()
                );
                println!(
                    "│  {} /review <file>     Review code file             │",
                    "👁️ ".bright_white()
                );
                println!(
                    "│  {} /plan <task>       Create task plan             │",
                    "📝".bright_white()
                );
                println!(
                    "{}",
                    "├──────────────────────────────────────────────────────┤".bright_cyan()
                );
                println!(
                    "│  {} @file              Reference file in message    │",
                    "📎".bright_white()
                );
                println!(
                    "│  {} exit               Exit interactive mode        │",
                    "🚪".bright_white()
                );
                println!(
                    "{}",
                    "╰──────────────────────────────────────────────────────╯".bright_cyan()
                );
                println!();
                println!(
                    "  {} Use @path/to/file to include file content in your message",
                    "💡".bright_yellow()
                );
                println!();
                continue;
            }

            if input == "/status" {
                let mode_str = match self.execution_mode() {
                    crate::config::ExecutionMode::Normal => "Normal",
                    crate::config::ExecutionMode::AutoEdit => "Auto-Edit",
                    crate::config::ExecutionMode::Yolo => "YOLO",
                    crate::config::ExecutionMode::Daemon => "Daemon",
                };
                println!("Messages in context: {}", self.messages.len());
                println!("Memory entries: {}", self.memory.len());
                println!("Estimated tokens: {}", self.memory.total_tokens());
                println!("Near limit: {}", self.memory.is_near_limit());
                println!("Current step: {}", self.loop_control.current_step());
                println!("Execution mode: {}", mode_str.bright_yellow());
                continue;
            }

            if input == "/stats" {
                self.show_session_stats();
                continue;
            }

            if input == "/compress" {
                match self.compress_context().await {
                    Ok(saved) => {
                        if saved > 0 {
                            println!("{} Saved {} tokens", "✓".bright_green(), saved);
                        }
                    }
                    Err(e) => println!("{} Compression error: {}", "❌".bright_red(), e),
                }
                continue;
            }

            if input == "/clear" {
                self.messages.retain(|m| m.role == "system");
                self.memory.clear();
                println!("Conversation cleared (system prompt retained)");
                continue;
            }

            if input == "/tools" {
                for tool in self.tools.list() {
                    println!("  - {}: {}", tool.name(), tool.description());
                }
                continue;
            }

            if input == "/mode" {
                use crate::config::ExecutionMode;
                let new_mode = self.cycle_execution_mode();
                let mode_desc = match new_mode {
                    ExecutionMode::Normal => "Normal - Ask for confirmation on all tools",
                    ExecutionMode::AutoEdit => "Auto-Edit - Auto-approve file operations",
                    ExecutionMode::Yolo => "YOLO - Execute all tools without confirmation",
                    ExecutionMode::Daemon => "Daemon - Permanent YOLO mode",
                };
                println!("{} Mode: {}", "🔄".bright_cyan(), mode_desc.bright_yellow());
                continue;
            }

            // Context management commands
            if input == "/context" || input == "/ctx" {
                self.show_context_stats();
                continue;
            }

            if input == "/context clear" || input == "/ctx clear" {
                self.clear_context();
                println!("{} Context cleared", "🗑️".bright_green());
                continue;
            }

            if input.starts_with("/context load ") || input.starts_with("/ctx load ") {
                let pattern = input
                    .strip_prefix("/context load ")
                    .or_else(|| input.strip_prefix("/ctx load "))
                    .unwrap()
                    .trim();
                match self.load_files_to_context(pattern).await {
                    Ok(count) => println!(
                        "{} Loaded {} files into context",
                        "📂".bright_green(),
                        count
                    ),
                    Err(e) => println!("{} Error loading files: {}", "❌".bright_red(), e),
                }
                continue;
            }

            if input == "/context reload" || input == "/ctx reload" {
                match self.reload_context().await {
                    Ok(count) => println!(
                        "{} Reloaded {} files into context",
                        "🔄".bright_green(),
                        count
                    ),
                    Err(e) => println!("{} Error reloading: {}", "❌".bright_red(), e),
                }
                continue;
            }

            if input == "/context copy" || input == "/ctx copy" {
                match self.copy_sources_to_clipboard() {
                    Ok(size) => {
                        println!("{} Copied {} chars to clipboard", "📋".bright_green(), size)
                    }
                    Err(e) => println!("{} Error copying: {}", "❌".bright_red(), e),
                }
                continue;
            }

            if input == "/memory" {
                let (entries, tokens, near_limit) = self.memory_stats();
                println!("Memory entries: {}", entries);
                println!("Estimated tokens: {}", tokens);
                println!("Context window: {}", self.memory.context_window());
                println!("Near limit: {}", near_limit);
                if !self.memory.is_empty() {
                    println!("\nRecent entries:");
                    println!("{}", self.memory.summary(3));
                }
                continue;
            }

            if input.starts_with("/review ") {
                let file_path = input.strip_prefix("/review ").unwrap().trim();
                match self.review(file_path).await {
                    Ok(_) => {}
                    Err(e) => println!("{} Error reviewing file: {}", "❌".bright_red(), e),
                }
                continue;
            }

            if input.starts_with("/analyze ") {
                let path = input.strip_prefix("/analyze ").unwrap().trim();
                match self.analyze(path).await {
                    Ok(_) => {}
                    Err(e) => println!("{} Error analyzing: {}", "❌".bright_red(), e),
                }
                continue;
            }

            if input.starts_with("/plan ") {
                let task = input.strip_prefix("/plan ").unwrap().trim();
                let context = self.memory.summary(5);
                let plan_prompt = Planner::create_plan(task, &context);
                match self.run_task(&plan_prompt).await {
                    Ok(_) => {}
                    Err(e) => println!("{} Error planning: {}", "❌".bright_red(), e),
                }
                continue;
            }

            // Expand @file references in input (Qwen Code style)
            let (expanded_input, included_files) = self.expand_file_references(input);
            if !included_files.is_empty() {
                println!(
                    "{} Included {} file(s):",
                    "📎".bright_cyan(),
                    included_files.len()
                );
                for file in &included_files {
                    println!("   {} {}", "→".bright_black(), file.bright_white());
                }
                println!();
            }

            // Display truncated preview for large pastes
            const LARGE_PASTE_THRESHOLD: usize = 3000;
            const PREVIEW_CHARS: usize = 200;

            if expanded_input.len() > LARGE_PASTE_THRESHOLD {
                let lines: Vec<&str> = expanded_input.lines().collect();
                let line_count = lines.len();
                let char_count = expanded_input.len();

                // Get first and last few characters for preview
                let start_preview: String = expanded_input.chars().take(PREVIEW_CHARS).collect();
                let end_preview: String = expanded_input
                    .chars()
                    .rev()
                    .take(PREVIEW_CHARS)
                    .collect::<String>()
                    .chars()
                    .rev()
                    .collect();

                println!("{} Large input detected:", "📋".bright_cyan());
                println!(
                    "   {} chars, {} lines",
                    char_count.to_string().bright_yellow(),
                    line_count.to_string().bright_yellow()
                );
                println!();
                println!("{}", "─".repeat(50).bright_black());
                println!("{}", start_preview.bright_white());
                println!("{}", "...".bright_black());
                println!("{}", end_preview.bright_white());
                println!("{}", "─".repeat(50).bright_black());
                println!();
            }

            match self.run_task(&expanded_input).await {
                Ok(_) => {}
                Err(e) => println!("{} Error: {}", "❌".bright_red(), e),
            }
        }

        Ok(())
    }

    /// Basic interactive mode (fallback when reedline unavailable)
    async fn interactive_basic(&mut self) -> Result<()> {
        use std::io::{self, Write};

        println!("{}", "🦊 Selfware Workshop (Basic Mode)".bright_cyan());
        println!("Type 'exit' to quit, '/help' for commands");

        // Detect if stdin is a TTY or piped
        use std::io::IsTerminal;
        let is_tty = std::io::stdin().is_terminal();

        loop {
            if is_tty {
                print!("🦊 ❯ ");
                io::stdout().flush()?;
            }

            let mut input = String::new();
            let bytes_read = io::stdin().read_line(&mut input)?;

            // EOF detection: read_line returns Ok(0) on EOF
            if bytes_read == 0 {
                break;
            }

            let input = input.trim();

            // Skip empty lines in non-interactive mode to avoid spurious tasks
            if input.is_empty() {
                if is_tty {
                    continue; // In TTY mode, just prompt again
                } else {
                    break; // In piped mode, empty line = done
                }
            }

            if input == "exit" || input == "quit" {
                break;
            }

            if input == "/help" {
                println!("Commands:");
                println!("  /help           - Show this help");
                println!("  /status         - Show agent status");
                println!("  /memory         - Show memory statistics");
                println!("  /clear          - Clear conversation history");
                println!("  /tools          - List available tools");
                println!("  /analyze <path> - Analyze codebase at path");
                println!("  /review <file>  - Review code in file");
                println!("  /plan <task>    - Create a plan for a task");
                println!("  exit            - Exit interactive mode");
                continue;
            }

            if input == "/status" {
                println!("Messages in context: {}", self.messages.len());
                println!("Memory entries: {}", self.memory.len());
                println!("Estimated tokens: {}", self.memory.total_tokens());
                println!("Near limit: {}", self.memory.is_near_limit());
                println!("Current step: {}", self.loop_control.current_step());
                continue;
            }

            if input == "/clear" {
                self.messages.retain(|m| m.role == "system");
                self.memory.clear();
                println!("Conversation cleared (system prompt retained)");
                continue;
            }

            if input == "/tools" {
                for tool in self.tools.list() {
                    println!("  - {}: {}", tool.name(), tool.description());
                }
                continue;
            }

            if input == "/memory" {
                let (entries, tokens, near_limit) = self.memory_stats();
                println!("Memory entries: {}", entries);
                println!("Estimated tokens: {}", tokens);
                println!("Context window: {}", self.memory.context_window());
                println!("Near limit: {}", near_limit);
                if !self.memory.is_empty() {
                    println!("\nRecent entries:");
                    println!("{}", self.memory.summary(3));
                }
                continue;
            }

            if input.starts_with("/review ") {
                let file_path = input.strip_prefix("/review ").unwrap().trim();
                match self.review(file_path).await {
                    Ok(_) => {}
                    Err(e) => println!("{} Error reviewing file: {}", "❌".bright_red(), e),
                }
                continue;
            }

            if input.starts_with("/analyze ") {
                let path = input.strip_prefix("/analyze ").unwrap().trim();
                match self.analyze(path).await {
                    Ok(_) => {}
                    Err(e) => println!("{} Error analyzing: {}", "❌".bright_red(), e),
                }
                continue;
            }

            if input.starts_with("/plan ") {
                let task = input.strip_prefix("/plan ").unwrap().trim();
                let context = self.memory.summary(5);
                let plan_prompt = Planner::create_plan(task, &context);
                match self.run_task(&plan_prompt).await {
                    Ok(_) => {}
                    Err(e) => println!("{} Error planning: {}", "❌".bright_red(), e),
                }
                continue;
            }

            match self.run_task(input).await {
                Ok(_) => {}
                Err(e) => println!("{} Error: {}", "❌".bright_red(), e),
            }
        }

        Ok(())
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
