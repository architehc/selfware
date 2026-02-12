use anyhow::{Context, Result};
use chrono::Utc;
use colored::*;
use serde_json::Value;
use tracing::{debug, info, warn};

use crate::analyzer::ErrorAnalyzer;
use crate::api::types::{Message, ToolCall};
use crate::api::{ApiClient, StreamChunk, ThinkingMode};
use crate::checkpoint::{
    capture_git_state, CheckpointManager, TaskCheckpoint, TaskStatus, ToolCallLog,
};
use crate::cognitive::{CognitiveState, CyclePhase};
use crate::config::Config;
use crate::memory::AgentMemory;
use crate::safety::SafetyChecker;
use crate::telemetry::{enter_agent_step, record_state_transition};
use crate::tool_parser::parse_tool_calls;
use crate::tools::ToolRegistry;
use crate::verification::{VerificationConfig, VerificationGate};

pub mod context;
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
    /// Generic recoverable error
    Recoverable(String),
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
            AgentError::Recoverable(msg) => write!(f, "{}", msg),
        }
    }
}

impl std::error::Error for AgentError {}

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
                        println!(); // End reasoning line
                    }
                    print!("{}", text);
                    io::stdout().flush().ok();
                    content.push_str(&text);
                }
                StreamChunk::Reasoning(text) => {
                    if !in_reasoning {
                        in_reasoning = true;
                        print!("{} ", "Thinking:".dimmed());
                    }
                    print!("{}", text.dimmed());
                    io::stdout().flush().ok();
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

        agent.current_checkpoint = Some(checkpoint);
        agent.checkpoint_manager = Some(checkpoint_manager);

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
            let task_id = self
                .current_checkpoint
                .as_ref()
                .map(|c| c.task_id.clone())
                .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

            let checkpoint = self.to_checkpoint(&task_id, task_description);
            manager.save(&checkpoint)?;
            self.current_checkpoint = Some(checkpoint);
            debug!("Checkpoint saved for task: {}", task_id);
        }
        Ok(())
    }

    /// Mark current task as completed
    fn complete_checkpoint(&mut self) -> Result<()> {
        if let Some(ref mut checkpoint) = self.current_checkpoint {
            checkpoint.set_status(TaskStatus::Completed);
            if let Some(ref manager) = self.checkpoint_manager {
                manager.save(checkpoint)?;
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

        while let Some(state) = self.loop_control.next_state() {
            match state {
                AgentState::Planning => {
                    let _span = enter_agent_step("Planning", 0);
                    record_state_transition("Start", "Planning");
                    println!("{}", "📋 Planning...".bright_yellow());

                    // Set cognitive state to Plan phase
                    self.cognitive_state.set_phase(CyclePhase::Plan);

                    // Plan returns true if the response contains tool calls
                    let has_tool_calls = self.plan().await?;

                    // Transition to Do phase
                    record_state_transition("Planning", "Executing");
                    self.cognitive_state.set_phase(CyclePhase::Do);
                    self.loop_control
                        .set_state(AgentState::Executing { step: 0 });

                    // If planning response contained tool calls, execute them now
                    if has_tool_calls {
                        println!("{} Executing...", format!("📝 Step {}", 1).bright_blue());
                        match self.execute_pending_tool_calls(&task_description).await {
                            Ok(completed) => {
                                if completed {
                                    record_state_transition("Executing", "Completed");
                                    println!("{}", "✅ Task completed!".bright_green());
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
                    println!(
                        "{} Executing...",
                        format!("📝 Step {}", step + 1).bright_blue()
                    );
                    match self.execute_step_with_logging(&task_description).await {
                        Ok(completed) => {
                            if completed {
                                record_state_transition("Executing", "Completed");
                                println!("{}", "✅ Task completed!".bright_green());
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
                            if e.downcast_ref::<AgentError>()
                                .map(|ae| matches!(ae, AgentError::ConfirmationRequired { .. }))
                                .unwrap_or(false)
                            {
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
                    println!("{}", "✅ Task completed successfully!".bright_green());
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

    /// Execute a step with tool call logging for checkpoints
    /// If `use_last_message` is true, process tool calls from the last assistant message
    /// instead of making a new API call (used after planning phase)
    async fn execute_step_with_logging(&mut self, _task_description: &str) -> Result<bool> {
        self.execute_step_internal(false).await
    }

    /// Execute tool calls from the last assistant message (after planning)
    async fn execute_pending_tool_calls(&mut self, _task_description: &str) -> Result<bool> {
        self.execute_step_internal(true).await
    }

    /// Internal execution logic
    /// If `use_last_message` is true, process tool calls from the last assistant message
    async fn execute_step_internal(&mut self, use_last_message: bool) -> Result<bool> {
        // For native function calling, we track tool_calls separately
        let mut native_tool_calls: Option<Vec<crate::api::types::ToolCall>> = None;

        let (content, reasoning_content) = if use_last_message {
            // Extract content from the last assistant message
            let last_msg = self
                .messages
                .iter()
                .rev()
                .find(|m| m.role == "assistant")
                .context("No previous assistant message found")?;
            debug!(
                "Using content from last assistant message ({} chars)",
                last_msg.content.len()
            );
            // Also check for native tool calls in the last message
            if self.config.agent.native_function_calling {
                native_tool_calls = last_msg.tool_calls.clone();
            }
            (last_msg.content.clone(), last_msg.reasoning_content.clone())
        } else {
            // Check compression before adding more context
            if self.compressor.should_compress(&self.messages) {
                info!("Context compression triggered");
                match self.compressor.compress(&self.client, &self.messages).await {
                    Ok(compressed) => {
                        self.messages = compressed;
                    }
                    Err(e) => {
                        warn!("Compression failed, using hard limit: {}", e);
                        self.messages = self.compressor.hard_compress(&self.messages);
                    }
                }
            }

            // Use streaming or regular chat based on config
            let (content, reasoning) = if self.config.agent.streaming {
                let (content, reasoning, stream_tool_calls) = self
                    .chat_streaming(
                        self.messages.clone(),
                        self.api_tools(),
                        ThinkingMode::Enabled,
                    )
                    .await?;

                // Handle native tool calls from stream
                if self.config.agent.native_function_calling && stream_tool_calls.is_some() {
                    native_tool_calls = stream_tool_calls.clone();
                    info!(
                        "Received {} native tool calls from stream",
                        native_tool_calls.as_ref().map(|t| t.len()).unwrap_or(0)
                    );
                }

                // Add assistant message to history
                self.messages.push(Message {
                    role: "assistant".to_string(),
                    content: content.clone(),
                    reasoning_content: reasoning.clone(),
                    tool_calls: native_tool_calls.clone(),
                    tool_call_id: None,
                    name: None,
                });

                (content, reasoning)
            } else {
                // Non-streaming path
                let response = self
                    .client
                    .chat(
                        self.messages.clone(),
                        self.api_tools(),
                        ThinkingMode::Enabled,
                    )
                    .await?;

                let choice = response
                    .choices
                    .into_iter()
                    .next()
                    .context("No response from model")?;

                let message = choice.message;
                let content = message.content.clone();
                let reasoning = message.reasoning_content.clone();

                // Check for native tool calls in response
                if self.config.agent.native_function_calling && message.tool_calls.is_some() {
                    native_tool_calls = message.tool_calls.clone();
                    info!(
                        "Received {} native tool calls from API",
                        native_tool_calls.as_ref().map(|t| t.len()).unwrap_or(0)
                    );
                }

                // Debug: print raw response content for troubleshooting
                debug!(
                    "Raw model response content ({} chars): {}",
                    content.len(),
                    content
                );

                // Verbose logging when SELFWARE_DEBUG is set
                if std::env::var("SELFWARE_DEBUG").is_ok() {
                    println!("{}", "=== DEBUG: Raw Model Response ===".bright_magenta());
                    println!("{}", content);
                    println!("{}", "=== END DEBUG ===".bright_magenta());
                }

                if content.is_empty() {
                    warn!("Model returned empty content!");
                }

                // Print reasoning if present (non-streaming only, streaming prints inline)
                if let Some(ref r) = reasoning {
                    println!("{} {}", "Thinking:".dimmed(), r.dimmed());
                    debug!("Reasoning content ({} chars): {}", r.len(), r);
                }

                // Add assistant message to history
                self.messages.push(Message {
                    role: "assistant".to_string(),
                    content: content.clone(),
                    reasoning_content: reasoning.clone(),
                    tool_calls: native_tool_calls.clone(),
                    tool_call_id: None,
                    name: None,
                });

                (content, reasoning)
            };

            (content, reasoning)
        };

        // Tool calls with their IDs (for native function calling)
        // Format: (name, args_str, tool_call_id)
        let mut tool_calls: Vec<(String, String, Option<String>)> = Vec::new();

        // Check for native function calling first (only if tool_calls is non-empty)
        if self.config.agent.native_function_calling
            && native_tool_calls.as_ref().is_some_and(|tc| !tc.is_empty())
        {
            let native_calls = native_tool_calls.as_ref().unwrap();
            info!("Using {} native tool calls from API", native_calls.len());
            for tc in native_calls {
                debug!(
                    "Native tool call: {} (id: {}) with args: {}",
                    tc.function.name, tc.id, tc.function.arguments
                );
                tool_calls.push((
                    tc.function.name.clone(),
                    tc.function.arguments.clone(),
                    Some(tc.id.clone()),
                ));
            }
        } else {
            // Fall back to parsing tool calls from content using robust multi-format parser
            // This happens when native FC is disabled OR the API returns empty tool_calls
            info!(
                "Falling back to XML parsing (native FC returned {} tool calls)",
                native_tool_calls.as_ref().map(|t| t.len()).unwrap_or(0)
            );
            debug!("Looking for tool calls with multi-format parser...");
            let parse_result = parse_tool_calls(&content);
            tool_calls = parse_result
                .tool_calls
                .iter()
                .map(|tc| {
                    debug!(
                        "Found tool call in content: {} with args: {}",
                        tc.tool_name, tc.arguments
                    );
                    (tc.tool_name.clone(), tc.arguments.to_string(), None)
                })
                .collect();

            // Log any parse errors
            for error in &parse_result.parse_errors {
                warn!("Tool parse error: {}", error);
            }

            // Also check reasoning content for tool calls (some models put tools there)
            if tool_calls.is_empty() {
                if let Some(ref reasoning_text) = reasoning_content {
                    let reasoning_result = parse_tool_calls(reasoning_text);
                    let reasoning_tools: Vec<(String, String, Option<String>)> = reasoning_result
                        .tool_calls
                        .iter()
                        .map(|tc| {
                            debug!(
                                "Found tool call in reasoning: {} with args: {}",
                                tc.tool_name, tc.arguments
                            );
                            (tc.tool_name.clone(), tc.arguments.to_string(), None)
                        })
                        .collect();
                    if !reasoning_tools.is_empty() {
                        info!(
                            "Found {} tool calls in reasoning content",
                            reasoning_tools.len()
                        );
                        tool_calls = reasoning_tools;
                    }
                }
            }
        }

        debug!("Total tool calls to execute: {}", tool_calls.len());

        // If no tool calls found, check if content looks like it should have them
        if tool_calls.is_empty()
            && (content.contains("<tool")
                || content.contains("tool_name")
                || content.contains("function"))
        {
            warn!("Content appears to contain tool-related keywords but no valid tool calls were parsed:");
            warn!(
                "Content preview: {}",
                &content.chars().take(500).collect::<String>()
            );
        }

        // Keep reasoning for use outside this block
        let _reasoning = reasoning_content;

        // Detect "intent to act" responses that should trigger follow-up
        // These are responses where the model says it will do something but doesn't use tools
        let intent_phrases = [
            "let me", "i'll ", "i will", "let's", "first,", "starting", "begin by", "going to",
            "need to", "start by", "help you",
        ];
        let content_lower = content.to_lowercase();
        let has_intent = intent_phrases.iter().any(|p| content_lower.contains(p));

        // If no tool calls but content suggests intent to act, prompt for action
        if tool_calls.is_empty() && has_intent && content.len() < 1000 && !use_last_message {
            info!("Detected intent without action, prompting model to use tools");
            println!(
                "{}",
                "🔄 Model described intent but didn't act - prompting for action..."
                    .bright_yellow()
            );
            self.messages.push(Message::user(
                "Please use the appropriate tools to take action now. Don't just describe what you'll do - actually execute the tools."
            ));
            return Ok(false); // Continue loop to get actual tool calls
        }

        if !tool_calls.is_empty() {
            // Execute tools with logging
            for (name, args_str, tool_call_id) in tool_calls {
                println!(
                    "{} Calling tool: {}",
                    "🔧".bright_blue(),
                    name.bright_cyan()
                );

                let start_time = std::time::Instant::now();
                let use_native_fc =
                    self.config.agent.native_function_calling && tool_call_id.is_some();

                // Safety check
                let call_id = tool_call_id
                    .clone()
                    .unwrap_or_else(|| format!("call_{}", uuid::Uuid::new_v4()));
                let fake_call = crate::api::types::ToolCall {
                    id: call_id.clone(),
                    call_type: "function".to_string(),
                    function: crate::api::types::ToolFunction {
                        name: name.clone(),
                        arguments: args_str.clone(),
                    },
                };

                if let Err(e) = self.safety.check_tool_call(&fake_call) {
                    let error_msg = format!("Safety check failed: {}", e);
                    println!("{} {}", "🚫".bright_red(), error_msg);

                    // Push result with appropriate message type
                    if use_native_fc {
                        self.messages.push(Message::tool(
                            serde_json::json!({"error": error_msg}).to_string(),
                            &call_id,
                        ));
                    } else {
                        self.messages.push(Message::user(format!(
                            "<tool_result><error>{}</error></tool_result>",
                            error_msg
                        )));
                    }

                    // Log failed tool call
                    if let Some(ref mut checkpoint) = self.current_checkpoint {
                        checkpoint.log_tool_call(ToolCallLog {
                            timestamp: Utc::now(),
                            tool_name: name.clone(),
                            arguments: args_str.clone(),
                            result: Some(error_msg),
                            success: false,
                            duration_ms: Some(start_time.elapsed().as_millis() as u64),
                        });
                    }
                    continue;
                }

                // Check execution mode for confirmation
                if self.needs_confirmation(&name) {
                    use std::io::{self, Write};

                    // Show tool call preview
                    let args_preview: String = args_str.chars().take(100).collect();
                    let args_display = if args_str.len() > 100 {
                        format!("{}...", args_preview)
                    } else {
                        args_preview
                    };

                    // In non-interactive mode, fail fast - don't silently skip tools
                    if !self.is_interactive() {
                        return Err(AgentError::ConfirmationRequired {
                            tool_name: name.clone(),
                        }
                        .into());
                    }

                    println!(
                        "{} Tool: {} Args: {}",
                        "⚠️".bright_yellow(),
                        name.bright_cyan(),
                        args_display.bright_white()
                    );
                    print!("{}", "Execute? [y/N/s(kip all)]: ".bright_yellow());
                    io::stdout().flush().ok();

                    let mut response = String::new();
                    if io::stdin().read_line(&mut response).is_ok() {
                        let response = response.trim().to_lowercase();
                        match response.as_str() {
                            "y" | "yes" => {
                                // Proceed with execution
                            }
                            "s" | "skip" => {
                                // Switch to yolo mode for this session
                                self.set_execution_mode(crate::config::ExecutionMode::Yolo);
                                println!(
                                    "{} Switched to YOLO mode for this session",
                                    "⚡".bright_yellow()
                                );
                            }
                            _ => {
                                // User rejected - skip this tool
                                let skip_msg = "Tool execution skipped by user";
                                println!("{} {}", "⏭️".bright_yellow(), skip_msg);

                                if use_native_fc {
                                    self.messages.push(Message::tool(
                                        serde_json::json!({"skipped": skip_msg}).to_string(),
                                        &call_id,
                                    ));
                                } else {
                                    self.messages.push(Message::user(format!(
                                        "<tool_result><skipped>{}</skipped></tool_result>",
                                        skip_msg
                                    )));
                                }
                                continue;
                            }
                        }
                    }
                }

                // Parse and execute
                let args: Value = match serde_json::from_str(&args_str) {
                    Ok(v) => v,
                    Err(e) => {
                        let err = format!("Invalid JSON arguments: {}", e);
                        println!("{} {}", "✗".bright_red(), err);

                        // Push result with appropriate message type
                        if use_native_fc {
                            self.messages.push(Message::tool(
                                serde_json::json!({"error": err}).to_string(),
                                &call_id,
                            ));
                        } else {
                            self.messages.push(Message::user(format!(
                                "<tool_result><error>{}</error></tool_result>",
                                err
                            )));
                        }

                        // Log failed tool call
                        if let Some(ref mut checkpoint) = self.current_checkpoint {
                            checkpoint.log_tool_call(ToolCallLog {
                                timestamp: Utc::now(),
                                tool_name: name.clone(),
                                arguments: args_str.clone(),
                                result: Some(err),
                                success: false,
                                duration_ms: Some(start_time.elapsed().as_millis() as u64),
                            });
                        }
                        continue;
                    }
                };

                debug!("Tool arguments: {}", args);

                let (success, result) = match self.tools.get(&name) {
                    Some(tool) => {
                        match tool.execute(args.clone()).await {
                            Ok(result) => {
                                println!("{} Tool succeeded", "✓".bright_green());
                                let result_str = serde_json::to_string(&result)?;

                                // Log successful tool call
                                if let Some(ref mut checkpoint) = self.current_checkpoint {
                                    checkpoint.log_tool_call(ToolCallLog {
                                        timestamp: Utc::now(),
                                        tool_name: name.clone(),
                                        arguments: args_str.clone(),
                                        result: Some(result_str.chars().take(1000).collect()),
                                        success: true,
                                        duration_ms: Some(start_time.elapsed().as_millis() as u64),
                                    });
                                }

                                // Run verification after file_edit tool
                                let verification_result = if name == "file_edit" {
                                    if let Some(path) = args.get("path").and_then(|v| v.as_str()) {
                                        info!("Running verification after file_edit on {}", path);
                                        self.cognitive_state.set_phase(CyclePhase::Verify);
                                        match self
                                            .verification_gate
                                            .verify_change(
                                                &[path.to_string()],
                                                &format!("file_edit:{}", path),
                                            )
                                            .await
                                        {
                                            Ok(report) => {
                                                if report.overall_passed {
                                                    self.cognitive_state
                                                        .episodic_memory
                                                        .what_worked(
                                                            "file_edit",
                                                            &format!(
                                                                "Edit to {} passed verification",
                                                                path
                                                            ),
                                                        );
                                                    println!("{}", report);
                                                    None
                                                } else {
                                                    self.cognitive_state
                                                        .episodic_memory
                                                        .what_failed(
                                                            "file_edit",
                                                            &format!(
                                                                "Edit to {} failed verification",
                                                                path
                                                            ),
                                                        );
                                                    println!("{}", report);
                                                    Some(format!("\n\n<verification_failed>\n{}\n</verification_failed>", report))
                                                }
                                            }
                                            Err(e) => {
                                                warn!("Verification failed to run: {}", e);
                                                None
                                            }
                                        }
                                    } else {
                                        None
                                    }
                                } else {
                                    None
                                };

                                // Enhance cargo_check output with error analysis
                                let enhanced_result = if name == "cargo_check"
                                    && result_str.contains("\"success\":false")
                                {
                                    self.enhance_cargo_errors(&result_str)
                                } else {
                                    result_str.clone()
                                };

                                // Combine result with verification if applicable
                                let final_result = match verification_result {
                                    Some(ver_msg) => format!("{}{}", enhanced_result, ver_msg),
                                    None => enhanced_result,
                                };

                                (true, final_result)
                            }
                            Err(e) => {
                                println!("{} Tool failed: {}", "✗".bright_red(), e);

                                // Log failed tool call
                                if let Some(ref mut checkpoint) = self.current_checkpoint {
                                    checkpoint.log_tool_call(ToolCallLog {
                                        timestamp: Utc::now(),
                                        tool_name: name.clone(),
                                        arguments: args_str.clone(),
                                        result: Some(e.to_string()),
                                        success: false,
                                        duration_ms: Some(start_time.elapsed().as_millis() as u64),
                                    });
                                }

                                // Record failure in cognitive state
                                self.cognitive_state
                                    .episodic_memory
                                    .what_failed(&name, &e.to_string());

                                (false, e.to_string())
                            }
                        }
                    }
                    None => {
                        let err = format!("Unknown tool: {}", name);
                        println!("{} {}", "✗".bright_red(), err);

                        // Log unknown tool call
                        if let Some(ref mut checkpoint) = self.current_checkpoint {
                            checkpoint.log_tool_call(ToolCallLog {
                                timestamp: Utc::now(),
                                tool_name: name.clone(),
                                arguments: args_str.clone(),
                                result: Some(err.clone()),
                                success: false,
                                duration_ms: Some(start_time.elapsed().as_millis() as u64),
                            });
                        }

                        (false, err)
                    }
                };

                // Push result with appropriate message type
                if use_native_fc {
                    // For native function calling, send JSON result with tool role
                    let result_json = if success {
                        result
                    } else {
                        serde_json::json!({"error": result}).to_string()
                    };
                    self.messages.push(Message::tool(result_json, &call_id));
                } else {
                    // For XML-based calling, wrap in tool_result tags
                    let formatted = if success {
                        format!("<tool_result>{}</tool_result>", result)
                    } else {
                        format!("<tool_result><error>{}</error></tool_result>", result)
                    };
                    self.messages.push(Message::user(formatted));
                }
            }

            Ok(false) // Continue loop
        } else {
            // No tool calls, task complete
            println!("{} {}", "Final answer:".bright_green(), content);
            // Note: assistant message was already added above when not using last message
            Ok(true)
        }
    }

    /// Plan phase - returns true if model wants to execute tools (should continue to execution)
    /// This now combines planning with initial tool extraction to avoid double API calls
    async fn plan(&mut self) -> Result<bool> {
        // Tools are embedded in system prompt - see WORKAROUND comment in Agent::new()
        debug!("Sending planning request to model...");
        let response = self
            .client
            .chat(
                self.messages.clone(),
                self.api_tools(),
                ThinkingMode::Enabled,
            )
            .await?;

        let choice = response
            .choices
            .into_iter()
            .next()
            .context("No response from model")?;

        let assistant_msg = choice.message;
        let content = &assistant_msg.content;

        // Debug logging for planning response
        debug!(
            "Planning response content ({} chars): {}",
            content.len(),
            content
        );

        // Verbose logging when SELFWARE_DEBUG is set
        if std::env::var("SELFWARE_DEBUG").is_ok() {
            println!("{}", "=== DEBUG: Planning Response ===".bright_magenta());
            println!("{}", content);
            println!("{}", "=== END DEBUG ===".bright_magenta());
        }

        if content.is_empty() {
            warn!("Model returned empty planning content!");
        }
        if let Some(ref reasoning) = assistant_msg.reasoning_content {
            debug!(
                "Planning reasoning ({} chars): {}",
                reasoning.len(),
                reasoning
            );
            if let Some(r) = &assistant_msg.reasoning_content {
                println!("{} {}", "Thinking:".dimmed(), r.dimmed());
            }
        }

        // Check if the planning response contains tool calls
        // For native function calling, check tool_calls field; otherwise parse from content
        let (has_tool_calls, native_tool_calls) =
            if self.config.agent.native_function_calling && assistant_msg.tool_calls.is_some() {
                let tool_calls = assistant_msg.tool_calls.as_ref().unwrap();
                info!(
                    "Planning response has {} native tool calls",
                    tool_calls.len()
                );
                (!tool_calls.is_empty(), assistant_msg.tool_calls.clone())
            } else {
                let parsed = !parse_tool_calls(content).tool_calls.is_empty();
                debug!("Planning response has tool calls (parsed): {}", parsed);
                (parsed, None)
            };

        self.messages.push(Message {
            role: "assistant".to_string(),
            content: content.clone(),
            reasoning_content: assistant_msg.reasoning_content,
            tool_calls: native_tool_calls,
            tool_call_id: None,
            name: None,
        });

        // Return whether there are tool calls to execute
        Ok(has_tool_calls)
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
                                println!("{}", "✅ Task completed!".bright_green());
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
                            if e.downcast_ref::<AgentError>()
                                .map(|ae| matches!(ae, AgentError::ConfirmationRequired { .. }))
                                .unwrap_or(false)
                            {
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
                    println!("{}", "✅ Task completed successfully!".bright_green());
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
