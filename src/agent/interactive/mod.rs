use anyhow::Result;
use colored::*;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use super::*;

mod helpers;
pub(crate) use helpers::*;

impl Agent {
    /// Start the interactive REPL loop (readline-based with tool-name autocomplete).
    ///
    /// Reads user input line-by-line, dispatches slash commands (e.g. `/help`,
    /// `/swarm`, `/copy`), and routes regular messages through `run_task`.
    /// Falls back to a basic `stdin` reader when the terminal is not interactive
    /// or reedline initialisation fails.
    pub async fn interactive(&mut self) -> Result<()> {
        use std::io::IsTerminal;
        if !std::io::stdin().is_terminal() {
            eprintln!("Terminal input unavailable, falling back to basic mode...");
            return self.interactive_basic().await;
        }

        use crate::input::{InputConfig, ReadlineResult, SelfwareEditor};

        let cancel = self.cancel_token();
        let _ = ctrlc::set_handler(move || {
            cancel.store(true, std::sync::atomic::Ordering::Relaxed);
        });

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
            "🦊 Selfware Interactive Mode".bright_cyan(),
            mode_indicator.bright_yellow()
        );
        self.show_startup_context();
        // Show context stats on startup (like /ctx)
        self.show_context_stats();
        // Discover user skills so /skills can list them and /<name> injects them.
        let skill_registry = crate::skills::SkillRegistry::discover();
        if !skill_registry.is_empty() {
            println!(
                "  {} skill(s) available — /skills to list",
                skill_registry.len()
            );
        }
        println!(
            "  Type {} for commands, {} for context, {} to quit",
            "/help".bright_cyan(),
            "/ctx".bright_cyan(),
            "exit".bright_cyan(),
        );

        let mut consecutive_errors = 0;
        const MAX_CONSECUTIVE_ERRORS: u32 = 3;
        let mut last_ctrl_c: Option<(Instant, u32)> = None; // (timestamp, tap_count)

        loop {
            // Check global shutdown flag (e.g. from SIGTERM)
            if crate::is_shutdown_requested() {
                println!("\n{}", "Shutdown requested, exiting...".bright_yellow());
                break;
            }

            // Auto-refresh stale files before prompting
            let refreshed = self.refresh_stale_context_files().await;
            if refreshed > 0 {
                println!(
                    "  {} Refreshed {} modified file{} in context",
                    "⟳".bright_cyan(),
                    refreshed,
                    if refreshed == 1 { "" } else { "s" }
                );
            }

            // Print status bar and update prompt with context usage before each input
            self.print_status_bar();
            let ctx_pct = self.context_usage_pct();
            let step = self.loop_control.current_step();
            editor.set_prompt_full_context(&self.config.model, step, ctx_pct);

            // Use block_in_place to prevent blocking the async runtime
            // while waiting for blocking I/O (stdin read from reedline)
            let input = match tokio::task::block_in_place(|| editor.read_line()) {
                Ok(ReadlineResult::Line(line)) => {
                    consecutive_errors = 0;
                    last_ctrl_c = None;
                    self.reset_cancellation();
                    line
                }
                Ok(ReadlineResult::Interrupt) => {
                    consecutive_errors = 0;
                    self.reset_cancellation();

                    // Triple-tap Ctrl+C for immediate force quit
                    // Double-tap for graceful exit
                    const DOUBLE_TAP_MS: u128 = 3000; // 3 seconds (was 1.5)
                    const FORCE_QUIT_TAPS: u32 = 3;

                    let now = Instant::now();
                    let tap_count = if let Some((last, count)) = last_ctrl_c {
                        if now.duration_since(last).as_millis() < DOUBLE_TAP_MS {
                            count + 1
                        } else {
                            1 // Reset if too slow
                        }
                    } else {
                        1
                    };

                    last_ctrl_c = Some((now, tap_count));

                    if tap_count >= FORCE_QUIT_TAPS {
                        // Force quit immediately
                        println!("\n\x1b[1m\x1b[91m⚠️  FORCE QUIT - Exiting immediately\x1b[0m");
                        std::process::exit(1);
                    } else if tap_count == 2 {
                        // Graceful exit on double-tap
                        println!("\n\x1b[1m\x1b[33m👋 Gracefully exiting... (saving checkpoint if needed)\x1b[0m");
                        break;
                    } else {
                        // First tap - show clear instructions
                        eprintln!("\n");
                        eprintln!("╔════════════════════════════════════════════════════════════╗");
                        eprintln!("║  \x1b[1m\x1b[97mINTERRUPT RECEIVED\x1b[0m                                       ║");
                        eprintln!("╠════════════════════════════════════════════════════════════╣");
                        eprintln!("║  Current task cancelled. Options:                          ║");
                        eprintln!("║                                                            ║");
                        eprintln!("║  • \x1b[1mCtrl+C again\x1b[0m      → Exit gracefully (within 3s)         ║");
                        eprintln!("║  • \x1b[1mCtrl+C ×3\x1b[0m         → Force quit immediately              ║");
                        eprintln!("║  • \x1b[1mType 'exit'\x1b[0m       → Exit at prompt                     ║");
                        eprintln!("║  • \x1b[1mType a command\x1b[0m    → Continue with new task             ║");
                        eprintln!("╚════════════════════════════════════════════════════════════╝");
                    }
                    continue;
                }
                Ok(ReadlineResult::Eof) => break,
                Ok(ReadlineResult::HostCommand(cmd)) => {
                    last_ctrl_c = None;
                    match cmd.as_str() {
                        "__toggle_yolo__" => {
                            use crate::config::ExecutionMode;
                            let new_mode = match self.execution_mode() {
                                ExecutionMode::Yolo => ExecutionMode::Normal,
                                _ => ExecutionMode::Yolo,
                            };
                            self.set_execution_mode(new_mode);
                            let label = match new_mode {
                                ExecutionMode::Yolo => "YOLO".bright_red(),
                                _ => "Normal".bright_green(),
                            };
                            println!("{} Mode: {}", "⚡".bright_yellow(), label);
                        }
                        "__cycle_mode__" | "__toggle_auto_edit__" => {
                            use crate::config::ExecutionMode;
                            let new_mode = self.cycle_execution_mode();
                            let (icon, label) = match new_mode {
                                ExecutionMode::Normal => ("🟢", "Normal".bright_green()),
                                ExecutionMode::AutoEdit => ("✏️", "Auto-Edit".bright_cyan()),
                                ExecutionMode::Yolo => ("⚡", "YOLO".bright_red()),
                                ExecutionMode::Daemon => ("🔄", "Daemon".bright_magenta()),
                            };
                            println!("{} Mode: {}", icon, label);
                        }
                        _ => {}
                    }
                    continue;
                }
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

            if is_exit_command(input) {
                break;
            }

            // Shell escape: !command runs via sh -c
            if input.starts_with('!') {
                let Some(cmd) = input.strip_prefix('!').map(str::trim) else {
                    println!("{} Usage: !<command>", "ℹ".bright_yellow());
                    continue;
                };
                if cmd.is_empty() {
                    println!("{} Usage: !<command>", "ℹ".bright_yellow());
                } else {
                    let (shell, flag) = crate::tools::shell_exec::default_shell();
                    let status = tokio::process::Command::new(shell)
                        .args([flag, cmd])
                        .stdout(std::process::Stdio::inherit())
                        .stderr(std::process::Stdio::inherit())
                        .stdin(std::process::Stdio::inherit())
                        .status()
                        .await;
                    match status {
                        Ok(s) if !s.success() => {
                            println!(
                                "{} exit code: {}",
                                "⚠".bright_yellow(),
                                s.code().unwrap_or(-1)
                            );
                        }
                        Err(e) => println!("{} Shell error: {}", "✗".bright_red(), e),
                        _ => {}
                    }
                }
                continue;
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
                    "│  {} /scan <path>       Index folder for RAG search  │",
                    "🔍".bright_white()
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
                    "│  {} /dream            Memory consolidation     │",
                    "🌙".bright_white()
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
                    "│  {} /diff              Git diff --stat              │",
                    "📊".bright_white()
                );
                println!(
                    "│  {} /git               Git status --short           │",
                    "📋".bright_white()
                );
                println!(
                    "│  {} /undo              Undo last file edit          │",
                    "↩ ".bright_white()
                );
                println!(
                    "│  {} /worktree enter    Create and enter worktree    │",
                    "🌳".bright_white()
                );
                println!(
                    "│  {} /worktree exit     Exit current worktree        │",
                    "🌲".bright_white()
                );
                println!(
                    "│  {} /worktree list     List all worktrees           │",
                    "📋".bright_white()
                );
                println!(
                    "│  {} /cost              Token usage & cost           │",
                    "💰".bright_white()
                );
                println!(
                    "│  {} /model             Model configuration          │",
                    "🤖".bright_white()
                );
                println!(
                    "│  {} /compact           Compress context (auto)      │",
                    "📦".bright_white()
                );
                println!(
                    "│  {} /compact micro     Fast local compression       │",
                    "📦".bright_white()
                );
                println!(
                    "│  {} /compact auto      LLM summarization            │",
                    "📦".bright_white()
                );
                println!(
                    "│  {} /compact full      Nuclear + file re-inject     │",
                    "📦".bright_white()
                );
                println!(
                    "│  {} /compact stats     Show compression stats       │",
                    "📦".bright_white()
                );
                println!(
                    "│  {} /verbose           Toggle verbose mode          │",
                    "📢".bright_white()
                );
                println!(
                    "│  {} /config            Show current config          │",
                    "⚙ ".bright_white()
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
                    "│  {} /swarm <task>      Run task with dev swarm      │",
                    "🐝".bright_white()
                );
                println!(
                    "│  {} /queue <msg>       Queue message for later      │",
                    "📨".bright_white()
                );
                println!(
                    "{}",
                    "├──────────────────────────────────────────────────────┤".bright_cyan()
                );
                println!(
                    "{}",
                    "├──────────────────────────────────────────────────────┤".bright_cyan()
                );
                println!(
                    "│  {} /vim               Toggle vim/emacs mode        │",
                    "⌨ ".bright_white()
                );
                println!(
                    "│  {} /copy              Copy last response           │",
                    "📋".bright_white()
                );
                println!(
                    "│  {} /restore           List/restore checkpoints     │",
                    "⏪".bright_white()
                );
                println!(
                    "│  {} /chat save <n>     Save chat session            │",
                    "💾".bright_white()
                );
                println!(
                    "│  {} /chat resume <n>   Resume saved chat            │",
                    "▶ ".bright_white()
                );
                println!(
                    "│  {} /chat list         List saved chats             │",
                    "📋".bright_white()
                );
                println!(
                    "│  {} /theme <name>      Switch color theme           │",
                    "🎨".bright_white()
                );
                println!(
                    "│  {} !<cmd>             Run shell command            │",
                    "💲".bright_white()
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
                    "├──────────────────────────────────────────────────────┤".bright_cyan()
                );
                println!(
                    "{}",
                    "│             ⌨  KEYBOARD SHORTCUTS                   │".bright_cyan()
                );
                println!(
                    "{}",
                    "├──────────────────────────────────────────────────────┤".bright_cyan()
                );
                println!("│  ESC           Interrupt running task               │");
                println!("│  Ctrl+C        Interrupt running task               │");
                println!("│  Ctrl+C ×2     Exit (double-tap at prompt)          │");
                println!("│  Ctrl+J        Insert newline (multi-line)          │");
                println!("│  Ctrl+Y        Toggle YOLO mode                     │");
                println!("│  Shift+Tab     Toggle Auto-Edit mode                │");
                println!("│  Ctrl+X        Open external editor ($EDITOR)       │");
                println!("│  Ctrl+L        Clear screen                         │");
                println!("│  Ctrl+R        Reverse history search               │");
                println!("│  Tab           Autocomplete / cycle suggestions     │");
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
                self.show_session_stats().await;
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

            if input.starts_with("/scan") {
                let path_arg = input.strip_prefix("/scan").map(str::trim).unwrap_or(".");
                if path_arg.is_empty() || path_arg == "help" {
                    println!("{} Usage: /scan <path>", "i".bright_yellow());
                    println!("  Index a folder or file for RAG semantic search.");
                    println!("  Example: /scan ./src");
                    continue;
                }
                self.handle_scan_command(path_arg).await;
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
                let Some(pattern) = input
                    .strip_prefix("/context load ")
                    .or_else(|| input.strip_prefix("/ctx load "))
                    .map(str::trim)
                else {
                    println!("{} Usage: /context load <glob>", "ℹ".bright_yellow());
                    continue;
                };
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
                match self.copy_sources_to_clipboard().await {
                    Ok(size) => {
                        println!("{} Copied {} chars to clipboard", "📋".bright_green(), size)
                    }
                    Err(e) => println!("{} Error copying: {}", "❌".bright_red(), e),
                }
                continue;
            }

            // === New slash commands ===

            if input == "/diff" {
                match tokio::process::Command::new("git")
                    .args(["diff", "--stat"])
                    .output()
                    .await
                {
                    Ok(out) => {
                        let stdout = String::from_utf8_lossy(&out.stdout);
                        if stdout.trim().is_empty() {
                            println!("{} No changes", "✓".bright_green());
                        } else {
                            println!("{}", stdout);
                        }
                    }
                    Err(e) => println!("{} git diff failed: {}", "✗".bright_red(), e),
                }
                continue;
            }

            if input == "/git" {
                match tokio::process::Command::new("git")
                    .args(["status", "--short", "--branch"])
                    .output()
                    .await
                {
                    Ok(out) => {
                        let stdout = String::from_utf8_lossy(&out.stdout);
                        println!("{}", stdout);
                    }
                    Err(e) => println!("{} git status failed: {}", "✗".bright_red(), e),
                }
                continue;
            }

            // Worktree commands
            if input.starts_with("/worktree") {
                self.handle_worktree_command(input).await;
                continue;
            }

            if input == "/undo" {
                if let Some(checkpoint) = self.edit_history.undo() {
                    let restored = restore_checkpoint_guarded(&checkpoint.files).await;
                    if restored == 0 {
                        println!(
                            "{} Undo: {} (no files to restore)",
                            "↩".bright_yellow(),
                            checkpoint.action.description()
                        );
                    } else {
                        println!(
                            "{} Undone: {} ({} file(s) restored)",
                            "↩".bright_green(),
                            checkpoint.action.description(),
                            restored
                        );
                    }
                } else {
                    println!("{} Nothing to undo", "ℹ".bright_yellow());
                }
                continue;
            }

            if input == "/cost" {
                let (prompt, completion) = output::get_total_tokens();
                let total = prompt + completion;
                println!();
                println!("  {} Token Usage", "📊".bright_cyan());
                println!("  Prompt:     {:>10}", prompt.to_string().bright_white());
                println!(
                    "  Completion: {:>10}",
                    completion.to_string().bright_white()
                );
                println!("  Total:      {:>10}", total.to_string().bright_cyan());
                let est_cost = (prompt as f64 * 3.0 + completion as f64 * 15.0) / 1_000_000.0;
                if est_cost > 0.001 {
                    println!(
                        "  Est. cost:  {:>10}",
                        format!("~${:.4}", est_cost).dimmed()
                    );
                }
                println!();
                continue;
            }

            if input == "/model" {
                println!();
                println!("  {} Model Configuration", "🤖".bright_cyan());
                println!("  Model:       {}", self.config.model.bright_white());
                println!("  Endpoint:    {}", self.config.endpoint.bright_white());
                println!(
                    "  Max tokens:  {}",
                    self.config.max_tokens.to_string().bright_white()
                );
                println!(
                    "  Temperature: {}",
                    self.config.temperature.to_string().bright_white()
                );
                println!(
                    "  Streaming:   {}",
                    if self.config.agent.streaming {
                        "yes".bright_green()
                    } else {
                        "no".bright_red()
                    }
                );
                println!(
                    "  Native FC:   {}",
                    if self.config.agent.native_function_calling {
                        "yes".bright_green()
                    } else {
                        "no".bright_red()
                    }
                );
                println!();
                continue;
            }

            // Three-Layer Context Compression commands
            if input == "/compact" {
                // Default: AutoCompact
                match self.compact_auto().await {
                    Ok(metrics) => {
                        println!("{} {}", "✓".bright_green(), metrics.summary());
                    }
                    Err(e) => {
                        println!("{} AutoCompact failed: {}", "✗".bright_red(), e);
                        println!("{} Falling back to MicroCompact...", "→".bright_yellow());
                        let metrics = self.compact_micro();
                        println!("{}", metrics.summary());
                    }
                }
                continue;
            }

            if input == "/compact micro" {
                let metrics = self.compact_micro();
                println!("{} {}", "✓".bright_green(), metrics.summary());
                continue;
            }

            if input == "/compact auto" {
                match self.compact_auto().await {
                    Ok(metrics) => {
                        println!("{} {}", "✓".bright_green(), metrics.summary());
                    }
                    Err(e) => {
                        println!("{} AutoCompact failed: {}", "✗".bright_red(), e);
                    }
                }
                continue;
            }

            if input == "/compact full" {
                match self.compact_full().await {
                    Ok(metrics) => {
                        println!("{} {}", "✓".bright_green(), metrics.summary());
                    }
                    Err(e) => {
                        println!("{} FullCompact failed: {}", "✗".bright_red(), e);
                    }
                }
                continue;
            }

            if input == "/compact stats" {
                println!();
                println!("{}", "📊 Context Compression Stats".bright_cyan());
                println!("{}", self.compression_stats());
                continue;
            }

            if input == "/verbose" {
                let new_verbose = !output::is_verbose();
                output::init(
                    output::is_compact(),
                    new_verbose,
                    output::should_show_tokens(),
                );
                println!(
                    "{} Verbose mode: {}",
                    "⚙".bright_cyan(),
                    if new_verbose {
                        "ON".bright_green()
                    } else {
                        "OFF".bright_red()
                    }
                );
                continue;
            }

            if input == "/last" {
                match self.retrieve_last_tool_output() {
                    Some(output) => {
                        println!();
                        println!(
                            "  {} Last Tool: {} ({}ms)",
                            ">>".bright_cyan(),
                            output.tool_name.bright_white(),
                            output.duration_ms.to_string().dimmed()
                        );
                        if !output.summary.is_empty() {
                            println!("  Summary: {}", output.summary);
                        }
                        let status = if output.success {
                            "success".bright_green()
                        } else {
                            "failed".bright_red()
                        };
                        println!("  Status:  {}", status);
                        if let Some(code) = output.exit_code {
                            println!("  Exit:    {}", code);
                        }
                        if !output.full_output.is_empty() {
                            println!("  {}", "Output:".dimmed());
                            let lines: Vec<&str> = output.full_output.lines().collect();
                            let show = lines.len().min(50);
                            for line in &lines[..show] {
                                println!("    {}", line);
                            }
                            if lines.len() > 50 {
                                println!(
                                    "    {} ({} more lines)",
                                    "...".dimmed(),
                                    lines.len() - 50
                                );
                            }
                        }
                        println!();
                    }
                    None => {
                        println!("{} No tool output captured yet.", "i".bright_yellow());
                    }
                }
                continue;
            }

            if input == "/debug" {
                self.print_execution_debug();
                continue;
            }

            if input == "/debug full" {
                self.print_execution_debug_with_options(true);
                continue;
            }

            if let Some(idx_str) = input.strip_prefix("/debug tool ") {
                match idx_str.trim().parse::<usize>() {
                    Ok(idx) => self.print_execution_debug_tool_detail(idx),
                    Err(_) => println!("{} Usage: /debug tool <number>", "ℹ".bright_yellow()),
                }
                continue;
            }

            if input == "/debug state" {
                self.print_task_state_debug();
                continue;
            }

            if input == "/debug-log" {
                self.print_session_debug_log().await;
                continue;
            }

            if input == "/debug-log full" {
                self.print_session_debug_log_with_options(true).await;
                continue;
            }

            if input == "/config" {
                println!();
                println!("  {} Current Configuration", "⚙".bright_cyan());
                let mode_str = match self.execution_mode() {
                    crate::config::ExecutionMode::Normal => "Normal",
                    crate::config::ExecutionMode::AutoEdit => "Auto-Edit",
                    crate::config::ExecutionMode::Yolo => "YOLO",
                    crate::config::ExecutionMode::Daemon => "Daemon",
                };
                println!("  Exec mode:   {}", mode_str.bright_yellow());
                println!("  Model:       {}", self.config.model.bright_white());
                println!(
                    "  Max tokens:  {}",
                    self.config.max_tokens.to_string().bright_white()
                );
                println!(
                    "  Compact:     {}",
                    if output::is_compact() { "yes" } else { "no" }
                );
                println!(
                    "  Verbose:     {}",
                    if output::is_verbose() { "yes" } else { "no" }
                );
                println!(
                    "  Show tokens: {}",
                    if output::should_show_tokens() {
                        "yes"
                    } else {
                        "no"
                    }
                );
                println!(
                    "  Streaming:   {}",
                    if self.config.agent.streaming {
                        "yes"
                    } else {
                        "no"
                    }
                );
                println!(
                    "  Native FC:   {}",
                    if self.config.agent.native_function_calling {
                        "yes"
                    } else {
                        "no"
                    }
                );
                println!("  Max iters:   {}", self.config.agent.max_iterations);
                println!();
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
                let Some(file_path) = input.strip_prefix("/review ").map(str::trim) else {
                    println!("{} Usage: /review <file>", "ℹ".bright_yellow());
                    continue;
                };
                match self.review(file_path).await {
                    Ok(_) => self.after_task_run().await,
                    Err(e) => println!("{} Error reviewing file: {}", "❌".bright_red(), e),
                }
                continue;
            }

            if input.starts_with("/analyze ") {
                let Some(path) = input.strip_prefix("/analyze ").map(str::trim) else {
                    println!("{} Usage: /analyze <path>", "ℹ".bright_yellow());
                    continue;
                };
                match self.analyze(path).await {
                    Ok(_) => self.after_task_run().await,
                    Err(e) => println!("{} Error analyzing: {}", "❌".bright_red(), e),
                }
                continue;
            }

            // ── /explain command ────────────────────────────────────
            if input == "/explain" {
                println!("{} Usage:", "ℹ".bright_yellow());
                println!("  /explain <path>                  Explain code in a file");
                println!("  /explain --level <level>         Set detail level (beginner|intermediate|advanced|expert)");
                println!("  /explain --curriculum <dir>      Generate a learning curriculum from a directory");
                continue;
            }

            if let Some(args) = input.strip_prefix("/explain ") {
                self.handle_explain_command(args.trim()).await;
                continue;
            }

            if input.starts_with("/plan ") {
                let Some(task) = input.strip_prefix("/plan ").map(str::trim) else {
                    println!("{} Usage: /plan <task>", "ℹ".bright_yellow());
                    continue;
                };
                let context = self.memory.summary(5);
                let plan_prompt = Planner::create_plan(task, &context);
                match self.run_task_with_queue(&plan_prompt).await {
                    Ok(_) => {}
                    Err(e) => println!("{} Error planning: {}", "❌".bright_red(), e),
                }
                continue;
            }

            if input == "/swarm" {
                println!(
                    "{} Usage: /swarm <task> (uses Architect/Coder/Tester/Reviewer orchestration)",
                    "ℹ".bright_yellow()
                );
                continue;
            }

            if input.starts_with("/swarm ") {
                let Some(task) = input.strip_prefix("/swarm ").map(str::trim) else {
                    println!("{} Usage: /swarm <task>", "ℹ".bright_yellow());
                    continue;
                };
                if task.is_empty() {
                    println!("{} Usage: /swarm <task>", "ℹ".bright_yellow());
                    continue;
                }
                match self.run_swarm_with_queue(task).await {
                    Ok(_) => {}
                    Err(e) => println!("{} Swarm error: {}", "❌".bright_red(), e),
                }
                continue;
            }

            if input == "/queue" {
                println!("{} Usage:", "📨".bright_cyan());
                println!("  /queue <message>  — Enqueue a message");
                println!("  /queue list       — Show queued messages");
                println!("  /queue clear      — Clear all queued messages");
                println!("  /queue drop <n>   — Remove message by index");
                println!("  {} pending message(s)", self.pending_messages.len());
                continue;
            }

            if input == "/queue list" {
                let msgs = &self.pending_messages;
                if msgs.is_empty() {
                    println!("{} Queue is empty.", "📋".bright_cyan());
                } else {
                    println!("{} Queued messages ({}):", "📋".bright_cyan(), msgs.len());
                    for (i, msg) in msgs.iter().enumerate() {
                        let preview = preview_with_ellipsis(&msg.content, QUEUE_LIST_PREVIEW_BYTES);
                        println!("  {}. {}", i + 1, preview,);
                    }
                }
                continue;
            }

            if input == "/queue clear" {
                let count = self.pending_messages.len();
                self.pending_messages.clear();
                println!(
                    "{} Cleared {} queued message(s).",
                    "📋".bright_cyan(),
                    count
                );
                continue;
            }

            if let Some(idx_str) = input.strip_prefix("/queue drop ") {
                if let Ok(idx) = idx_str.trim().parse::<usize>() {
                    let idx = idx.saturating_sub(1); // 1-based to 0-based
                    if idx < self.pending_messages.len() {
                        let removed = self
                            .pending_messages
                            .remove(idx)
                            .expect("index checked above");
                        let preview =
                            preview_with_ellipsis(&removed.content, QUEUE_DROP_PREVIEW_BYTES);
                        println!(
                            "{} Removed message {}: {}",
                            "📋".bright_cyan(),
                            idx + 1,
                            preview,
                        );
                    } else {
                        println!(
                            "{} Invalid index. Use '/queue list' to see messages.",
                            "❌".bright_red()
                        );
                    }
                } else {
                    println!("{} Usage: /queue drop <number>", "ℹ".bright_yellow());
                }
                continue;
            }

            if input.starts_with("/queue ") {
                let Some(msg) = input.strip_prefix("/queue ").map(str::trim) else {
                    println!("{} Usage: /queue <message>", "ℹ".bright_yellow());
                    continue;
                };
                if msg.is_empty() {
                    println!("{} Usage: /queue <message>", "ℹ".bright_yellow());
                } else {
                    self.enqueue_pending_message(msg);
                    println!(
                        "{} Queued ({} pending)",
                        "📨".bright_green(),
                        self.pending_messages.len()
                    );
                }
                continue;
            }

            // /vim - Toggle vim/emacs mode
            if input == "/vim" {
                match editor.toggle_vim_mode() {
                    Ok(mode) => {
                        let label = match mode {
                            crate::input::InputMode::Vi => "Vi".bright_yellow(),
                            crate::input::InputMode::Emacs => "Emacs".bright_green(),
                        };
                        println!("{} Input mode: {}", "⌨".bright_cyan(), label);
                    }
                    Err(e) => println!("{} Failed to toggle mode: {}", "✗".bright_red(), e),
                }
                continue;
            }

            // /copy - Copy last response to clipboard
            if input == "/copy" {
                if self.last_assistant_response.is_empty() {
                    println!("{} No response to copy", "ℹ".bright_yellow());
                } else {
                    match Self::copy_text_to_clipboard(&self.last_assistant_response).await {
                        Ok(()) => {
                            let len = self.last_assistant_response.len();
                            println!("{} Copied {} chars to clipboard", "📋".bright_green(), len);
                        }
                        Err(e) => println!("{} Copy failed: {}", "✗".bright_red(), e),
                    }
                }
                continue;
            }

            // /restore - List/restore edit checkpoints
            if input == "/restore" {
                let timeline = self.edit_history.timeline();
                if timeline.is_empty() {
                    println!("{} No edit checkpoints available", "ℹ".bright_yellow());
                } else {
                    println!();
                    println!("  {} Edit History", "⏪".bright_cyan());
                    for (i, entry) in timeline.iter().enumerate() {
                        let icon = if entry.is_current {
                            "●".bright_green()
                        } else {
                            "○".bright_cyan()
                        };
                        println!(
                            "  {} {} {} - {}",
                            icon,
                            format!("[{}]", i).bright_white(),
                            entry.timestamp.format("%H:%M:%S").to_string().dimmed(),
                            entry.action.description().bright_white()
                        );
                    }
                    println!();
                    println!(
                        "  {} Use {} to restore a checkpoint",
                        "💡".bright_yellow(),
                        "/restore <n>".bright_cyan()
                    );
                    println!();
                }
                continue;
            }

            if input.starts_with("/restore ") {
                let Some(idx_str) = input.strip_prefix("/restore ").map(str::trim) else {
                    println!("{} Usage: /restore <number>", "ℹ".bright_yellow());
                    continue;
                };
                if let Ok(idx) = idx_str.parse::<usize>() {
                    let timeline = self.edit_history.timeline();
                    if idx < timeline.len() {
                        let checkpoint_id = timeline[idx].id;
                        if let Some(checkpoint) = self.edit_history.goto(checkpoint_id) {
                            let restored = restore_checkpoint_guarded(&checkpoint.files).await;
                            println!(
                                "{} Restored checkpoint {} ({} file(s))",
                                "⏪".bright_green(),
                                idx,
                                restored
                            );
                        } else {
                            println!("{} Failed to navigate to checkpoint", "✗".bright_red());
                        }
                    } else {
                        println!(
                            "{} Invalid checkpoint index (max: {})",
                            "✗".bright_red(),
                            timeline.len().saturating_sub(1)
                        );
                    }
                } else {
                    println!("{} Usage: /restore <number>", "ℹ".bright_yellow());
                }
                continue;
            }

            // /plan - Enter structured plan mode
            if input == "/plan" {
                self.enter_plan_mode();
                println!();
                println!("{}", "🔍 Plan Mode Enabled".bright_cyan().bold());
                println!("{}", "─".repeat(50).bright_black());
                println!(
                    "{} In planning mode, only read-only tools are available:",
                    "ℹ".bright_yellow()
                );
                println!("   • file_read, grep_search, glob_find");
                println!("   • directory_tree, symbol_search, tool_search");
                println!();
                println!(
                    "{} Type your request to analyze the codebase.",
                    "→".bright_black()
                );
                println!(
                    "{} The agent will create a structured plan.",
                    "→".bright_black()
                );
                println!(
                    "{} Use /execute to approve, or /modify to cancel.",
                    "→".bright_black()
                );
                println!();
                continue;
            }

            // ── Guarded checkpoint restore (shared by /undo and /restore <n>) ──

            /// Restore a checkpoint's files with integrity guards: drift detection
            /// (skip files that changed since the checkpoint — undo must never clobber
            /// newer work), corrupt-snapshot rejection, and atomic temp+rename writes.
            /// Returns the number of files actually restored.
            async fn restore_checkpoint_guarded(
                files: &std::collections::HashMap<
                    std::path::PathBuf,
                    crate::session::edit_history::FileSnapshot,
                >,
            ) -> usize {
                let mut restored = 0;
                for (path, snapshot) in files {
                    let current_hash = match tokio::fs::read_to_string(path).await {
                        Ok(content) => crate::session::edit_history::compute_hash(&content),
                        Err(_) => snapshot.hash.clone(), // missing file is safe to restore
                    };
                    if current_hash != snapshot.hash {
                        let snapshot_ok =
                            crate::session::edit_history::compute_hash(&snapshot.content)
                                == snapshot.hash;
                        if !snapshot_ok {
                            println!(
                                "  {} Skipped {} (snapshot hash mismatch — corrupt checkpoint)",
                                "✗".bright_red(),
                                path.display().to_string().bright_white()
                            );
                            continue;
                        }
                        println!(
                "  {} Skipped {} (file changed since checkpoint — restore would clobber newer edits)",
                "⚠".bright_yellow(),
                path.display().to_string().bright_white()
            );
                        continue;
                    }
                    // Atomic write: temp file + rename, never a torn write.
                    let tmp = path.with_extension(format!(
                        "{}.undo-tmp",
                        path.extension().and_then(|e| e.to_str()).unwrap_or("bak")
                    ));
                    let written = tokio::fs::write(&tmp, &snapshot.content).await.is_ok()
                        && tokio::fs::rename(&tmp, path).await.is_ok();
                    if !written {
                        let _ = tokio::fs::remove_file(&tmp).await;
                    }
                    if written {
                        println!(
                            "  {} Restored {}",
                            "✓".bright_green(),
                            path.display().to_string().bright_white()
                        );
                        restored += 1;
                    }
                }
                restored
            }

            // /execute - Approve plan and execute
            if input == "/execute" {
                if !self.is_in_plan_mode() {
                    println!("{} Not in plan mode. Use /plan first.", "ℹ".bright_yellow());
                } else if self.is_plan_approved() {
                    println!("{} Plan is already being executed.", "ℹ".bright_yellow());
                } else {
                    self.approve_plan();
                    println!();
                    println!("{}", "✅ Plan Approved".bright_green().bold());
                    println!("{}", "─".repeat(50).bright_black());
                    if let Some(plan) = self.get_plan() {
                        println!("{}", plan.format().bright_white());
                    }
                    println!(
                        "{} The agent will now execute the plan.",
                        "⚡".bright_yellow()
                    );
                    println!();
                }
                continue;
            }

            // /modify - Exit plan mode without executing
            if input == "/modify" {
                if !self.is_in_plan_mode() {
                    println!("{} Not in plan mode.", "ℹ".bright_yellow());
                } else {
                    self.clear_plan();
                    println!();
                    println!("{}", "📝 Plan Cancelled".bright_yellow().bold());
                    println!("{}", "─".repeat(50).bright_black());
                    println!("{} Exited plan mode.", "ℹ".bright_yellow());
                    println!(
                        "{} You can now make new requests or enter /plan again.",
                        "→".bright_black()
                    );
                    println!();
                }
                continue;
            }

            // /hooks - Show registered hooks
            if input == "/hooks" {
                if self.hook_registry.is_empty() {
                    println!("{} No hooks registered", "ℹ".bright_yellow());
                    println!(
                        "  {} Add hooks in selfware.toml under [[hooks]]",
                        "→".bright_black()
                    );
                } else {
                    println!(
                        "\n  {} Registered Hooks ({})",
                        "🔗".bright_cyan(),
                        self.hook_registry.len()
                    );
                    // We can't easily iterate hooks without exposing them,
                    // so just show the count
                    println!(
                        "  {} {} hook(s) configured via selfware.toml",
                        "→".bright_black(),
                        self.hook_registry.len()
                    );
                    println!();
                }
                continue;
            }

            // /think - Toggle thinking mode
            if input == "/think" || input.starts_with("/think ") {
                let arg = input
                    .strip_prefix("/think")
                    .expect("checked by starts_with above")
                    .trim();
                match arg {
                    "" | "toggle" => {
                        // Toggle between Enabled and Disabled
                        let current = self
                            .config
                            .extra_body
                            .as_ref()
                            .and_then(|eb| eb.get("enable_thinking"))
                            .and_then(|v| v.as_bool())
                            .unwrap_or(true);
                        let new_state = !current;
                        let eb = self.config.extra_body.get_or_insert_with(Default::default);
                        eb.insert(
                            "enable_thinking".to_string(),
                            serde_json::Value::Bool(new_state),
                        );
                        if new_state {
                            println!("{} Thinking enabled", "🧠".bright_cyan());
                        } else {
                            println!(
                                "{} Thinking disabled (faster responses)",
                                "⚡".bright_green()
                            );
                        }
                    }
                    "on" => {
                        let eb = self.config.extra_body.get_or_insert_with(Default::default);
                        eb.insert("enable_thinking".to_string(), serde_json::Value::Bool(true));
                        println!("{} Thinking enabled", "🧠".bright_cyan());
                    }
                    "off" => {
                        let eb = self.config.extra_body.get_or_insert_with(Default::default);
                        eb.insert(
                            "enable_thinking".to_string(),
                            serde_json::Value::Bool(false),
                        );
                        println!(
                            "{} Thinking disabled (faster responses)",
                            "⚡".bright_green()
                        );
                    }
                    _ => {
                        println!("{} Usage: /think [on|off|toggle]", "ℹ".bright_yellow());
                    }
                }
                continue;
            }

            // /chat commands
            if input.starts_with("/chat save ") {
                let Some(name) = input.strip_prefix("/chat save ").map(str::trim) else {
                    println!("{} Usage: /chat save <name>", "ℹ".bright_yellow());
                    continue;
                };
                if name.is_empty() {
                    println!("{} Usage: /chat save <name>", "ℹ".bright_yellow());
                } else {
                    match self
                        .chat_store
                        .save(name, &self.messages, &self.config.model)
                    {
                        Ok(()) => println!("{} Chat '{}' saved", "💾".bright_green(), name),
                        Err(e) => println!("{} Save failed: {}", "✗".bright_red(), e),
                    }
                }
                continue;
            }

            if input.starts_with("/chat resume ") {
                let Some(name) = input.strip_prefix("/chat resume ").map(str::trim) else {
                    println!("{} Usage: /chat resume <name>", "ℹ".bright_yellow());
                    continue;
                };
                if name.is_empty() {
                    println!("{} Usage: /chat resume <name>", "ℹ".bright_yellow());
                } else {
                    match self.chat_store.load(name) {
                        Ok(chat) => {
                            self.messages = chat.messages;

                            // Restore memory system from recovered messages so that
                            // memory stats, token counts, and context are consistent.
                            self.memory.clear();
                            for msg in &self.messages {
                                if msg.role != "system" {
                                    self.memory.add_message(msg);
                                }
                            }

                            println!(
                                "{} Resumed chat '{}' ({} messages, model: {})",
                                "▶".bright_green(),
                                name,
                                self.messages.len(),
                                chat.model.bright_white()
                            );
                        }
                        Err(e) => println!("{} Resume failed: {}", "✗".bright_red(), e),
                    }
                }
                continue;
            }

            if input == "/chat list" {
                match self.chat_store.list() {
                    Ok(chats) => {
                        if chats.is_empty() {
                            println!("{} No saved chats", "ℹ".bright_yellow());
                        } else {
                            println!();
                            println!("  {} Saved Chats", "💬".bright_cyan());
                            for chat in &chats {
                                println!(
                                    "  {} {} ({} msgs, {}, {})",
                                    "●".bright_cyan(),
                                    chat.name.bright_white(),
                                    chat.message_count,
                                    chat.model.dimmed(),
                                    chat.saved_at.format("%Y-%m-%d %H:%M").to_string().dimmed()
                                );
                            }
                            println!();
                        }
                    }
                    Err(e) => println!("{} Error listing chats: {}", "✗".bright_red(), e),
                }
                continue;
            }

            if input.starts_with("/chat delete ") {
                let Some(name) = input.strip_prefix("/chat delete ").map(str::trim) else {
                    println!("{} Usage: /chat delete <name>", "ℹ".bright_yellow());
                    continue;
                };
                if name.is_empty() {
                    println!("{} Usage: /chat delete <name>", "ℹ".bright_yellow());
                } else {
                    match self.chat_store.delete(name) {
                        Ok(()) => println!("{} Chat '{}' deleted", "🗑️".bright_green(), name),
                        Err(e) => println!("{} Delete failed: {}", "✗".bright_red(), e),
                    }
                }
                continue;
            }

            if input == "/chat" {
                println!();
                println!("  {} Chat Commands", "💬".bright_cyan());
                println!(
                    "  {} /chat save <name>    Save current session",
                    "→".bright_black()
                );
                println!(
                    "  {} /chat resume <name>  Resume a saved chat",
                    "→".bright_black()
                );
                println!(
                    "  {} /chat list           List all saved chats",
                    "→".bright_black()
                );
                println!(
                    "  {} /chat delete <name>  Delete a saved chat",
                    "→".bright_black()
                );
                println!();
                continue;
            }

            // /dream - Memory consolidation (dream system)
            if input == "/dream" || input == "/dream status" {
                use crate::cognitive::dream::DreamConfig;
                use crate::cognitive::dream_subprocess::get_dream_status;

                let dream_config = DreamConfig::new();
                let status = get_dream_status(&dream_config).await;

                println!();
                println!("  {} Dream System Status", "🌙".bright_cyan());
                println!("  {}", "─".repeat(40).bright_black());

                // Last dream info
                if let Some(ts) = status.last_dream_timestamp {
                    let dt = chrono::DateTime::from_timestamp(ts as i64, 0)
                        .map(|d| d.format("%Y-%m-%d %H:%M").to_string())
                        .unwrap_or_else(|| "Unknown".to_string());
                    println!("  Last dream: {}", dt.bright_white());
                } else {
                    println!("  Last dream: {}", "Never".dimmed());
                }

                // Dream count
                println!(
                    "  Total dreams: {}",
                    status.dream_count.to_string().bright_white()
                );

                // Sessions since last dream
                println!(
                    "  Sessions since last: {}",
                    status.sessions_since_last_dream.to_string().bright_white()
                );

                // Until next dream
                println!();
                if status.is_running {
                    println!(
                        "  Status: {}",
                        "🔄 Consolidation in progress".bright_yellow()
                    );
                } else if status.hours_until_next == 0 && status.sessions_until_next == 0 {
                    println!("  Status: {}", "✓ Ready for next dream".bright_green());
                } else {
                    println!("  Until next dream:",);
                    if status.hours_until_next > 0 {
                        println!(
                            "    {} hours remaining",
                            status.hours_until_next.to_string().bright_yellow()
                        );
                    }
                    if status.sessions_until_next > 0 {
                        println!(
                            "    {} sessions remaining",
                            status.sessions_until_next.to_string().bright_yellow()
                        );
                    }
                }

                println!();
                println!(
                    "  {} Use {} to force consolidation",
                    "💡".bright_yellow(),
                    "/dream force".bright_cyan()
                );
                println!();
                continue;
            }

            if input == "/dream force" {
                use crate::cognitive::memory_system::DreamIntegratedMemorySystem;

                println!(
                    "  {} Force-triggering dream consolidation...",
                    "🌙".bright_cyan()
                );

                let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
                let dream_system = DreamIntegratedMemorySystem::new(&cwd);

                match dream_system.force_dream(&cwd).await {
                    Ok(result) => {
                        if result.success {
                            println!();
                            println!("  {} Dream completed successfully", "✓".bright_green());
                            println!(
                                "  Phases completed: {}",
                                result.phases_completed.len().to_string().bright_white()
                            );
                            println!(
                                "  Memories consolidated: {}",
                                result.memories_consolidated.to_string().bright_white()
                            );
                            println!(
                                "  Memories pruned: {}",
                                result.memories_pruned.to_string().bright_white()
                            );
                            println!(
                                "  Duration: {}s",
                                result.duration_secs.to_string().bright_white()
                            );
                            if !result.errors.is_empty() {
                                println!();
                                println!("  {} Warnings:", "⚠".bright_yellow());
                                for error in &result.errors {
                                    println!("    • {}", error.dimmed());
                                }
                            }
                        } else {
                            println!();
                            println!("  {} Dream failed", "✗".bright_red());
                            for error in &result.errors {
                                println!("    • {}", error.bright_red());
                            }
                        }
                        println!();
                    }
                    Err(e) => {
                        println!("  {} Dream error: {}", "✗".bright_red(), e);
                    }
                }
                continue;
            }

            // /theme - Switch color theme
            if input == "/theme" {
                let themes = crate::ui::theme::available_themes();
                let current = crate::ui::theme::current_theme_id();
                println!();
                println!("  {} Available Themes", "🎨".bright_cyan());
                for name in &themes {
                    let id = crate::ui::theme::theme_from_name(name);
                    let marker = if id == Some(current) {
                        "●".bright_green()
                    } else {
                        "○".dimmed()
                    };
                    println!("  {} {}", marker, name.bright_white());
                }
                println!();
                println!(
                    "  {} Use {} to switch",
                    "💡".bright_yellow(),
                    "/theme <name>".bright_cyan()
                );
                println!();
                continue;
            }

            if input.starts_with("/theme ") {
                let Some(name) = input.strip_prefix("/theme ").map(str::trim) else {
                    println!("{} Usage: /theme <name>", "ℹ".bright_yellow());
                    continue;
                };
                match crate::ui::theme::theme_from_name(name) {
                    Some(id) => {
                        crate::ui::theme::set_theme(id);
                        println!(
                            "{} Theme set to: {}",
                            "🎨".bright_green(),
                            name.bright_white()
                        );
                    }
                    None => {
                        println!(
                            "{} Unknown theme '{}'. Use /theme to see available themes.",
                            "✗".bright_red(),
                            name
                        );
                    }
                }
                continue;
            }

            // /skills - list discovered user skills
            if input == "/skills" {
                if skill_registry.is_empty() {
                    println!("No skills found in ~/.selfware/skills or ./.selfware/skills");
                } else {
                    println!("Skills:");
                    for skill in skill_registry.list() {
                        println!("  /{:<12} - {}", skill.name, skill.description);
                    }
                }
                continue;
            }

            // /<skill-name> - inject a discovered user skill as instructions
            if let Some(name) = input.strip_prefix('/') {
                if let Some(skill) = skill_registry.get(name) {
                    self.messages.push(Message::system(format!(
                        "The user invoked the /{} skill. Follow these instructions:\n\n{}",
                        skill.name, skill.content
                    )));
                    println!(
                        "  Skill '{}' loaded — instructions added to context.",
                        skill.name
                    );
                    continue;
                }
            }

            // Unknown slash command — reject with a pointer to the command
            // list instead of forwarding it to the LLM as a paid chat
            // message. Absolute paths (/tmp/foo.rs) fall through to chat.
            if looks_like_slash_command(input) {
                reject_unknown_slash_command(input);
                continue;
            }

            // Expand @file references in input (Qwen Code style)
            let (expanded_input, included_files) = self.expand_file_references(input).await;
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

                // Ask for confirmation before submitting large input
                print!("Submit this input? [Y/n] ");
                std::io::Write::flush(&mut std::io::stdout())?;
                let mut confirm = String::new();
                // Use block_in_place to prevent blocking the async runtime
                tokio::task::block_in_place(|| std::io::stdin().read_line(&mut confirm))?;
                let confirm = confirm.trim().to_lowercase();
                if confirm == "n" || confirm == "no" {
                    println!("Input cancelled.");
                    continue;
                }
            }

            match self.run_task_with_queue(&expanded_input).await {
                Ok(_) => {}
                Err(e) => println!("{} Error: {}", "❌".bright_red(), e),
            }
        }

        // Auto-save conversation/session on exit so history isn't lost.
        if let Err(e) = self.save_checkpoint("interactive session exit") {
            warn!("Failed to auto-save session on exit: {}", e);
        }

        // Clean up any managed background processes before exiting
        crate::tools::process::cleanup_all_processes().await;

        Ok(())
    }

    async fn run_task_with_queue(&mut self, task: &str) -> Result<()> {
        let esc_guard = spawn_esc_listener(
            self.cancel_token(),
            self.esc_pause_token(),
            self.esc_pause_ack_token(),
        );
        let result = self.run_task(task).await;
        let queued = coalesce_pending_messages(esc_guard.stop().await);
        // Add messages typed during generation to the pending queue
        for msg in queued {
            self.enqueue_pending_message_entry(msg);
        }
        self.after_task_run().await;
        result
    }

    async fn run_swarm_with_queue(&mut self, task: &str) -> Result<()> {
        let esc_guard = spawn_esc_listener(
            self.cancel_token(),
            self.esc_pause_token(),
            self.esc_pause_ack_token(),
        );
        let result = self.run_swarm_task(task).await;
        let queued = coalesce_pending_messages(esc_guard.stop().await);
        for msg in queued {
            self.enqueue_pending_message_entry(msg);
        }
        self.after_task_run().await;
        result
    }

    async fn after_task_run(&mut self) {
        let interrupted = self.is_cancelled();
        self.reset_cancellation();
        if !interrupted {
            self.drain_pending_messages().await;
        }
    }

    async fn drain_pending_messages(&mut self) {
        while let Some(mut queued) = self.pending_messages.pop_front() {
            while let Some(next) = self.pending_messages.front() {
                let within_window = next.queued_at.saturating_duration_since(queued.queued_at)
                    <= INTERACTIVE_QUEUE_COALESCE_WINDOW;
                if queued.origin == PendingMessageOrigin::InteractiveQueue
                    && next.origin == PendingMessageOrigin::InteractiveQueue
                    && within_window
                {
                    let next = self
                        .pending_messages
                        .pop_front()
                        .expect("just verified with pending_messages.front()");
                    if !queued.content.is_empty() {
                        queued.content.push('\n');
                    }
                    queued.content.push_str(&next.content);
                    queued.queued_at = next.queued_at;
                    continue;
                }
                break;
            }

            if is_effectively_empty_message(&queued.content) {
                continue;
            }

            let preview = preview_with_ellipsis(&queued.content, QUEUE_DRAIN_PREVIEW_BYTES);
            println!("{} Queued: {}", "📨".bright_cyan(), preview);

            if queued.content.starts_with('/') {
                // Slash commands typed mid-generation must NOT be sent raw
                // to the LLM.  Skip them here; the user can re-enter them
                // at the next prompt where the interactive command
                // dispatcher will handle them properly.
                println!(
                    "{} Skipped queued slash command (re-enter at prompt): {}",
                    "ℹ".bright_yellow(),
                    preview
                );
            } else if let Err(e) = self.run_task(&queued.content).await {
                println!("{} Error: {}", "❌".bright_red(), e);
            }

            let interrupted = self.is_cancelled();
            self.reset_cancellation();
            if interrupted {
                break;
            }
        }
    }

    fn enqueue_pending_message_entry(&mut self, msg: PendingMessage) {
        if self.pending_messages.len() >= MAX_PENDING_MESSAGES {
            let _ = self.pending_messages.pop_front();
            println!(
                "{} Queue full ({}). Dropped oldest queued message.",
                "⚠".bright_yellow(),
                MAX_PENDING_MESSAGES
            );
        }
        self.pending_messages.push_back(msg);
    }

    fn enqueue_pending_message(&mut self, msg: &str) {
        self.enqueue_pending_message_entry(PendingMessage::new(
            msg.to_string(),
            PendingMessageOrigin::ManualQueue,
            Instant::now(),
        ));
    }

    // ── /explain implementation ──────────────────────────────────────
    async fn handle_explain_command(&mut self, args: &str) {
        use crate::cognitive::learning::{CodeExplainer, ConceptExtractor, ExplainModeConfig};

        // /explain --level <level>
        if let Some(level_str) = args
            .strip_prefix("--level ")
            .or_else(|| args.strip_prefix("--level="))
        {
            let level_str = level_str.trim();
            match level_str.to_lowercase().as_str() {
                "beginner" => {
                    self.explanation_level = ExplanationLevel::Beginner;
                    println!(
                        "{} Explanation level set to {} ({})",
                        ">>".bright_cyan(),
                        "Beginner".bright_green(),
                        ExplanationLevel::Beginner.description()
                    );
                }
                "intermediate" => {
                    self.explanation_level = ExplanationLevel::Intermediate;
                    println!(
                        "{} Explanation level set to {} ({})",
                        ">>".bright_cyan(),
                        "Intermediate".bright_green(),
                        ExplanationLevel::Intermediate.description()
                    );
                }
                "advanced" => {
                    self.explanation_level = ExplanationLevel::Advanced;
                    println!(
                        "{} Explanation level set to {} ({})",
                        ">>".bright_cyan(),
                        "Advanced".bright_green(),
                        ExplanationLevel::Advanced.description()
                    );
                }
                "expert" => {
                    self.explanation_level = ExplanationLevel::Expert;
                    println!(
                        "{} Explanation level set to {} ({})",
                        ">>".bright_cyan(),
                        "Expert".bright_green(),
                        ExplanationLevel::Expert.description()
                    );
                }
                _ => {
                    println!(
                        "{} Unknown level '{}'. Use: beginner, intermediate, advanced, expert",
                        "!!".bright_red(),
                        level_str
                    );
                }
            }
            return;
        }

        // /explain --curriculum <dir>
        if let Some(dir_str) = args
            .strip_prefix("--curriculum ")
            .or_else(|| args.strip_prefix("--curriculum="))
        {
            let dir_path = std::path::Path::new(dir_str.trim());
            if !tokio::fs::try_exists(dir_path).await.unwrap_or(false) {
                println!(
                    "{} Directory not found: {}",
                    "!!".bright_red(),
                    dir_str.trim()
                );
                return;
            }
            if !tokio::fs::metadata(dir_path)
                .await
                .map(|m| m.is_dir())
                .unwrap_or(false)
            {
                println!("{} Not a directory: {}", "!!".bright_red(), dir_str.trim());
                return;
            }

            let extractor = ConceptExtractor::new();
            let mut all_concepts = Vec::new();

            if let Ok(mut entries) = tokio::fs::read_dir(dir_path).await {
                while let Ok(Some(entry)) = entries.next_entry().await {
                    let path = entry.path();
                    if path.extension().and_then(|e| e.to_str()) == Some("rs") {
                        if let Ok(content) = tokio::fs::read_to_string(&path).await {
                            let concepts = extractor.extract_from_code(&content, &path);
                            all_concepts.extend(concepts);
                        }
                    }
                }
            }

            if all_concepts.is_empty() {
                println!(
                    "{} No concepts found in {}",
                    "ℹ".bright_yellow(),
                    dir_str.trim()
                );
                return;
            }

            let curriculum = extractor
                .generate_curriculum(&all_concepts, &format!("Curriculum: {}", dir_str.trim()));

            println!();
            println!(
                "  {} {}",
                ">>".bright_cyan(),
                curriculum.title.bright_white().bold()
            );
            println!(
                "  {} Total estimated time: {} minutes",
                ">>".bright_cyan(),
                curriculum.total_minutes
            );
            println!(
                "  {} Concepts found: {}",
                ">>".bright_cyan(),
                curriculum.concepts.len()
            );
            println!();

            for lesson in curriculum.suggested_order() {
                println!(
                    "  {} Lesson {}: {}",
                    ">>".bright_cyan(),
                    lesson.order,
                    lesson.title.bright_green()
                );
                println!(
                    "     {} (~{} min)",
                    lesson.description.dimmed(),
                    lesson.estimated_minutes
                );
                for cid in &lesson.concepts {
                    if let Some(concept) = curriculum.get_concept(cid) {
                        println!(
                            "     - {} [{}]",
                            concept.name.bright_yellow(),
                            format!("{:?}", concept.difficulty).dimmed()
                        );
                    }
                }
                println!();
            }
            return;
        }

        // /explain <path> — explain a file
        let file_path = std::path::Path::new(args);
        if !tokio::fs::try_exists(file_path).await.unwrap_or(false) {
            println!("{} File not found: {}", "!!".bright_red(), args);
            return;
        }

        let content = match tokio::fs::read_to_string(file_path).await {
            Ok(c) => c,
            Err(e) => {
                println!("{} Cannot read file: {}", "!!".bright_red(), e);
                return;
            }
        };

        let config = ExplainModeConfig {
            level: self.explanation_level,
            ..ExplainModeConfig::default()
        };
        let mut explainer = CodeExplainer::new().with_config(config);
        let explanation = explainer.explain(&content);

        println!();
        println!(
            "  {} Explaining: {}",
            ">>".bright_cyan(),
            args.bright_white().bold()
        );
        println!("  {} Level: {:?}", ">>".bright_cyan(), explanation.level);
        println!();

        if !explanation.concepts.is_empty() {
            println!("  {} Concepts:", ">>".bright_cyan());
            for concept in &explanation.concepts {
                println!("     - {}", concept.bright_yellow());
            }
            println!();
        }

        if !explanation.explanation.is_empty() {
            println!("  {} Overview:", ">>".bright_cyan());
            for line in explanation.explanation.lines() {
                println!("     {}", line);
            }
            println!();
        }

        if !explanation.line_explanations.is_empty() {
            println!("  {} Line-by-line:", ">>".bright_cyan());
            for le in &explanation.line_explanations {
                println!(
                    "     {:>4} | {}",
                    le.line_number.to_string().dimmed(),
                    le.code.trim()
                );
                println!("          {}", le.explanation.bright_green());
                if !le.concepts.is_empty() {
                    println!("          concepts: {}", le.concepts.join(", ").dimmed());
                }
            }
            println!();
        }

        if !explanation.related_topics.is_empty() {
            println!("  {} Related topics:", ">>".bright_cyan());
            for topic in &explanation.related_topics {
                println!("     - {}", topic.bright_blue());
            }
            println!();
        }
    }

    // ── /worktree implementation ─────────────────────────────────────
    async fn handle_worktree_command(&mut self, input: &str) {
        use colored::Colorize;

        let args = input.strip_prefix("/worktree").map(str::trim).unwrap_or("");

        if args.is_empty() || args == "list" {
            // List worktrees
            match tokio::process::Command::new("git")
                .args(["worktree", "list", "--porcelain"])
                .output()
                .await
            {
                Ok(out) => {
                    let stdout = String::from_utf8_lossy(&out.stdout);
                    if stdout.trim().is_empty() {
                        println!("{} No worktrees found", "ℹ".bright_yellow());
                    } else {
                        println!("{} Git Worktrees:", "🌳".bright_cyan());
                        println!();

                        // Parse and display worktrees
                        let mut current_path: Option<String> = None;
                        let mut current_branch: Option<String> = None;
                        let mut is_detached = false;

                        for line in stdout.lines() {
                            if line.is_empty() {
                                // End of worktree entry, print it
                                if let Some(path) = current_path.take() {
                                    let branch_str = if is_detached {
                                        "(detached)".dimmed()
                                    } else {
                                        current_branch
                                            .as_deref()
                                            .unwrap_or("unknown")
                                            .bright_green()
                                    };
                                    println!(
                                        "  {} {} ({})",
                                        "📁".bright_white(),
                                        path.bright_white(),
                                        branch_str
                                    );
                                }
                                current_branch = None;
                                is_detached = false;
                            } else if let Some(path) = line.strip_prefix("worktree ") {
                                current_path = Some(path.to_string());
                            } else if let Some(branch) = line.strip_prefix("branch refs/heads/") {
                                current_branch = Some(branch.to_string());
                            } else if line == "detached" {
                                is_detached = true;
                            }
                        }

                        // Print last entry if exists
                        if let Some(path) = current_path {
                            let branch_str = if is_detached {
                                "(detached)".dimmed()
                            } else {
                                current_branch
                                    .as_deref()
                                    .unwrap_or("unknown")
                                    .bright_green()
                            };
                            println!(
                                "  {} {} ({})",
                                "📁".bright_white(),
                                path.bright_white(),
                                branch_str
                            );
                        }

                        println!();
                        println!("{} Usage:", "💡".bright_yellow());
                        println!("  /worktree enter [branch]  - Create and enter worktree");
                        println!("  /worktree exit            - Exit and return to main repo");
                    }
                }
                Err(e) => println!("{} Failed to list worktrees: {}", "✗".bright_red(), e),
            }
            return;
        }

        if args.starts_with("enter") {
            let branch_arg = args.strip_prefix("enter").map(str::trim).unwrap_or("");

            // Generate a unique worktree name
            let timestamp = chrono::Local::now().format("%Y%m%d_%H%M%S");
            let worktree_name = format!("worktree_{}", timestamp);

            // Get git root
            let git_root = match tokio::process::Command::new("git")
                .args(["rev-parse", "--show-toplevel"])
                .output()
                .await
            {
                Ok(out) if out.status.success() => {
                    String::from_utf8_lossy(&out.stdout).trim().to_string()
                }
                _ => {
                    println!("{} Not in a git repository", "✗".bright_red());
                    return;
                }
            };

            let worktree_path = format!("{}/.selfware/worktrees/{}", git_root, worktree_name);

            // Create parent directory
            if let Err(e) =
                tokio::fs::create_dir_all(format!("{}/.selfware/worktrees", git_root)).await
            {
                println!(
                    "{} Failed to create worktree directory: {}",
                    "✗".bright_red(),
                    e
                );
                return;
            }

            // Build git worktree add command
            let mut cmd_args = vec!["worktree", "add"];
            if branch_arg.is_empty() {
                cmd_args.push("--detach");
            }
            cmd_args.push(&worktree_path);
            if !branch_arg.is_empty() {
                cmd_args.push(branch_arg);
            }

            println!(
                "{} Creating worktree at {}...",
                "🌳".bright_cyan(),
                worktree_path.dimmed()
            );

            match tokio::process::Command::new("git")
                .args(&cmd_args)
                .output()
                .await
            {
                Ok(out) if out.status.success() => {
                    // Change to the worktree directory
                    match std::env::set_current_dir(&worktree_path) {
                        Ok(_) => {
                            let branch_display = if branch_arg.is_empty() {
                                "(detached)".dimmed()
                            } else {
                                branch_arg.bright_green()
                            };
                            println!(
                                "{} Entered worktree: {}",
                                "✓".bright_green(),
                                worktree_path.bright_white()
                            );
                            println!("  Branch: {}", branch_display);
                            println!();
                            println!(
                                "{} Working directory changed. Use '/worktree exit' to return.",
                                "💡".bright_yellow()
                            );
                        }
                        Err(e) => {
                            println!(
                                "{} Worktree created but failed to change directory: {}",
                                "⚠".bright_yellow(),
                                e
                            );
                            println!("  You can manually cd to: {}", worktree_path);
                        }
                    }
                }
                Ok(out) => {
                    let stderr = String::from_utf8_lossy(&out.stderr);
                    println!(
                        "{} Failed to create worktree: {}",
                        "✗".bright_red(),
                        stderr.trim()
                    );
                }
                Err(e) => println!("{} Failed to execute git worktree: {}", "✗".bright_red(), e),
            }
            return;
        }

        if args == "exit" {
            // First, find if we're in a worktree
            let current_dir = match std::env::current_dir() {
                Ok(d) => d,
                Err(e) => {
                    println!(
                        "{} Failed to get current directory: {}",
                        "✗".bright_red(),
                        e
                    );
                    return;
                }
            };

            // Find git root (main repo)
            let git_root = match tokio::process::Command::new("git")
                .args(["rev-parse", "--show-toplevel"])
                .output()
                .await
            {
                Ok(out) if out.status.success() => {
                    String::from_utf8_lossy(&out.stdout).trim().to_string()
                }
                _ => {
                    println!("{} Not in a git repository", "✗".bright_red());
                    return;
                }
            };

            let current_dir_str = current_dir.to_string_lossy();
            let is_worktree = current_dir_str != git_root && current_dir_str.starts_with(&git_root);

            if !is_worktree {
                println!("{} Not currently in a worktree", "ℹ".bright_yellow());
                println!("  Current: {}", current_dir_str.bright_white());
                println!("  Git root: {}", git_root.bright_white());
                return;
            }

            // Get the worktree path before we change
            let worktree_path = current_dir_str.to_string();

            // Change back to git root
            match std::env::set_current_dir(&git_root) {
                Ok(_) => {
                    println!("{} Exited worktree", "✓".bright_green());
                    println!("  Previous: {}", worktree_path.dimmed());
                    println!("  Current: {}", git_root.bright_white());

                    // Ask if user wants to remove the worktree
                    println!();
                    println!("{} Remove the worktree? (y/N)", "?".bright_cyan());

                    // We can't easily do interactive input here, so just suggest the tool
                    println!("  Use the tool to remove: enter_worktree with remove=true");
                    println!("  Or run: git worktree remove {}", worktree_path.dimmed());
                }
                Err(e) => {
                    println!("{} Failed to change directory: {}", "✗".bright_red(), e);
                }
            }
            return;
        }

        println!("{} Unknown worktree command: {}", "✗".bright_red(), args);
        println!("  Available commands:");
        println!("    /worktree list   - List all worktrees");
        println!("    /worktree enter [branch] - Create and enter worktree");
        println!("    /worktree exit   - Exit current worktree");
    }

    fn print_execution_debug(&self) {
        self.print_execution_debug_with_options(false);
    }

    fn print_execution_debug_with_options(&self, full: bool) {
        println!();
        println!("  {} Execution Debug", ">>".bright_cyan());

        if let Some(checkpoint) = &self.current_checkpoint {
            println!("  Task:   {}", checkpoint.task_description.bright_white());
            println!("  Task ID: {}", checkpoint.task_id.dimmed());
            println!(
                "  Status: {:?} | step {} | {} tool call(s) | {} error(s)",
                checkpoint.status,
                checkpoint.current_step,
                checkpoint.tool_calls.len(),
                checkpoint.errors.len()
            );

            if checkpoint.tool_calls.is_empty() {
                println!("  No tool calls captured for this task yet.");
            } else {
                let total = checkpoint.tool_calls.len();
                let start = if full { 0 } else { total.saturating_sub(10) };
                if full {
                    println!("  Showing all {} tool call(s) with full details", total);
                } else {
                    println!("  Showing tool calls {}-{} of {}", start + 1, total, total);
                }

                for (index, call) in checkpoint.tool_calls.iter().enumerate().skip(start) {
                    self.print_execution_debug_tool_call(index, call, full);
                }
            }

            if !checkpoint.errors.is_empty() {
                println!("  Recent errors:");
                for error in checkpoint.errors.iter().rev().take(5).rev() {
                    let recovered = if error.recovered {
                        "recovered".bright_green()
                    } else {
                        "unrecovered".bright_red()
                    };
                    println!(
                        "    step {} | {} | {}",
                        error.step,
                        recovered,
                        error.timestamp.format("%H:%M:%S").to_string().dimmed()
                    );
                    println!("      {}", error.error);
                }
            }
        } else {
            println!("  No active checkpoint/tool history for this session.");
        }

        if let Some(last) = self.retrieve_last_tool_output() {
            println!("  Last tool summary: {}", last.summary);
        }

        self.print_task_state_summary();

        println!();
    }

    fn print_execution_debug_tool_detail(&self, one_based_index: usize) {
        println!();
        println!("  {} Execution Debug Detail", ">>".bright_cyan());

        let Some(checkpoint) = &self.current_checkpoint else {
            println!("  No active checkpoint/tool history for this session.");
            println!();
            return;
        };

        if checkpoint.tool_calls.is_empty() {
            println!("  No tool calls captured for this task yet.");
            println!();
            return;
        }

        let Some((index, call)) = one_based_index
            .checked_sub(1)
            .and_then(|idx| checkpoint.tool_calls.get(idx).map(|call| (idx, call)))
        else {
            println!(
                "  {} Invalid tool call number. Use /debug to list available entries.",
                "ℹ".bright_yellow()
            );
            println!();
            return;
        };

        self.print_execution_debug_tool_call(index, call, true);
        println!();
    }

    fn print_execution_debug_tool_call(
        &self,
        index: usize,
        call: &crate::checkpoint::ToolCallLog,
        full: bool,
    ) {
        let _lock = crate::output::OUTPUT_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let status = if call.success {
            "success".bright_green()
        } else {
            "failed".bright_red()
        };
        let duration = call
            .duration_ms
            .map(|ms| format!("{}ms", ms))
            .unwrap_or_else(|| "n/a".to_string());

        println!(
            "  {}. {} {} ({}) at {}",
            index + 1,
            call.tool_name.bright_white(),
            status,
            duration.dimmed(),
            call.timestamp.format("%H:%M:%S").to_string().dimmed()
        );
        print_debug_args_block(&call.arguments);
        print_debug_result_block(call.result.as_deref(), full);
    }

    fn print_task_state_summary(&self) {
        let _lock = crate::output::OUTPUT_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        println!("  Task State");
        println!(
            "    tracked_files={} stale_files={} task_notes={}",
            self.file_tracker.read_state.len(),
            self.file_tracker.stale_files.len(),
            self.task_state_notes.len()
        );

        if !self.file_tracker.read_state.is_empty() {
            for (path, state) in self.file_tracker.read_state.iter().take(10) {
                println!(
                    "    - {} ({} lines, unchanged_reads={})",
                    path, state.total_lines, state.unchanged_read_count
                );
            }
        }

        if !self.file_tracker.stale_files.is_empty() {
            for path in self.file_tracker.stale_files.iter().take(10) {
                println!("    * stale: {}", path);
            }
        }

        if !self.task_state_notes.is_empty() {
            println!("    Recent notes:");
            for note in self.task_state_notes.iter().rev().take(5).rev() {
                println!("      {}", note);
            }
        }
    }

    fn print_task_state_debug(&self) {
        // NOTE: print_task_state_summary acquires OUTPUT_LOCK internally,
        // so we only lock the surrounding println! calls here.
        {
            let _lock = crate::output::OUTPUT_LOCK
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            println!();
            println!("  {} Task State", ">>".bright_cyan());
        }
        self.print_task_state_summary();
        {
            let _lock = crate::output::OUTPUT_LOCK
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            println!();
        }
    }

    /// Copy text to the system clipboard (shared helper in util.rs).
    async fn copy_text_to_clipboard(text: &str) -> Result<()> {
        crate::util::copy_to_clipboard(text).await
    }

    /// Basic interactive mode (fallback when reedline unavailable)
    async fn interactive_basic(&mut self) -> Result<()> {
        use std::io::{self, Write};

        println!("{}", "🦊 Selfware Workshop (Basic Mode)".bright_cyan());
        println!("Type 'exit' to quit, '/help' for commands");

        // Detect if stdin is a TTY or piped
        use std::io::IsTerminal;
        let is_tty = std::io::stdin().is_terminal();

        // Discover user skills so /skills can list them and /<name> injects them.
        let skill_registry = crate::skills::SkillRegistry::discover();

        loop {
            if is_tty {
                print!("🦊 ❯ ");
                io::stdout().flush()?;
            }

            let mut input = String::new();
            // Use block_in_place to prevent blocking the async runtime
            let bytes_read = tokio::task::block_in_place(|| io::stdin().read_line(&mut input))?;

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

            if is_exit_command(input) {
                break;
            }

            if input == "/help" {
                println!("Commands:");
                println!("  /help           - Show this help");
                println!("  /status         - Show agent status");
                println!("  /memory         - Show memory statistics");
                println!("  /clear          - Clear conversation history");
                println!("  /tools          - List available tools");
                println!("  /last           - Show the last tool output");
                println!("  /debug          - Show current task debug state");
                println!("  /debug full     - Show full args/results for the current task");
                println!("  /debug tool <n> - Show one tool call with full details");
                println!("  /debug state    - Show task-state memory");
                println!("  /debug-log      - Show the persistent session log");
                println!("  /debug-log full - Show full recent session log entries");
                println!("  /scan <path>    - Index folder for RAG search");
                println!("  /analyze <path> - Analyze codebase at path");
                println!("  /review <file>  - Review code in file");
                println!("  /explain <path> - Explain code in a file");
                println!("  /plan <task>    - Create a plan for a task");
                println!("  /swarm <task>   - Run task with dev swarm");
                println!("  /queue <msg>    - Queue a message");
                println!("  /queue list     - Show queued messages");
                println!("  /queue clear    - Clear all queued messages");
                println!("  /queue drop <n> - Remove message by index");
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

            if input.starts_with("/scan") {
                let path_arg = input.strip_prefix("/scan").map(str::trim).unwrap_or(".");
                if path_arg.is_empty() || path_arg == "help" {
                    println!("Usage: /scan <path>");
                    println!("  Index a folder or file for RAG semantic search.");
                    continue;
                }
                self.handle_scan_command(path_arg).await;
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

            if input == "/debug" {
                self.print_execution_debug();
                continue;
            }

            if input == "/debug full" {
                self.print_execution_debug_with_options(true);
                continue;
            }

            if let Some(idx_str) = input.strip_prefix("/debug tool ") {
                match idx_str.trim().parse::<usize>() {
                    Ok(idx) => self.print_execution_debug_tool_detail(idx),
                    Err(_) => println!("{} Usage: /debug tool <number>", "ℹ".bright_yellow()),
                }
                continue;
            }

            if input == "/debug state" {
                self.print_task_state_debug();
                continue;
            }

            if input == "/debug-log" {
                self.print_session_debug_log().await;
                continue;
            }

            if input == "/debug-log full" {
                self.print_session_debug_log_with_options(true).await;
                continue;
            }

            if input.starts_with("/review ") {
                let Some(file_path) = input.strip_prefix("/review ").map(str::trim) else {
                    println!("{} Usage: /review <file>", "ℹ".bright_yellow());
                    continue;
                };
                match self.review(file_path).await {
                    Ok(_) => self.after_task_run().await,
                    Err(e) => println!("{} Error reviewing file: {}", "❌".bright_red(), e),
                }
                continue;
            }

            if input.starts_with("/analyze ") {
                let Some(path) = input.strip_prefix("/analyze ").map(str::trim) else {
                    println!("{} Usage: /analyze <path>", "ℹ".bright_yellow());
                    continue;
                };
                match self.analyze(path).await {
                    Ok(_) => self.after_task_run().await,
                    Err(e) => println!("{} Error analyzing: {}", "❌".bright_red(), e),
                }
                continue;
            }

            // ── /explain command (headless) ─────────────────────────
            if input == "/explain" {
                println!("{} Usage:", "ℹ".bright_yellow());
                println!("  /explain <path>                  Explain code in a file");
                println!("  /explain --level <level>         Set detail level (beginner|intermediate|advanced|expert)");
                println!("  /explain --curriculum <dir>      Generate a learning curriculum from a directory");
                continue;
            }

            if input.starts_with("/explain ") {
                let args = input
                    .strip_prefix("/explain ")
                    .expect("checked by starts_with above")
                    .trim();
                self.handle_explain_command(args).await;
                continue;
            }

            if input == "/plan" {
                let enabled = self.toggle_plan_mode();
                if enabled {
                    println!("Plan mode ON — tool calls will be proposed but not executed");
                } else {
                    println!("Plan mode OFF — tool calls will execute normally");
                }
                continue;
            }

            if input.starts_with("/plan ") {
                let Some(task) = input.strip_prefix("/plan ").map(str::trim) else {
                    println!("{} Usage: /plan <task>", "ℹ".bright_yellow());
                    continue;
                };
                let context = self.memory.summary(5);
                let plan_prompt = Planner::create_plan(task, &context);
                match self.run_task_with_queue(&plan_prompt).await {
                    Ok(_) => {}
                    Err(e) => println!("{} Error planning: {}", "❌".bright_red(), e),
                }
                continue;
            }

            if input == "/swarm" {
                println!("{} Usage: /swarm <task>", "ℹ".bright_yellow());
                continue;
            }

            if input.starts_with("/swarm ") {
                let Some(task) = input.strip_prefix("/swarm ").map(str::trim) else {
                    println!("{} Usage: /swarm <task>", "ℹ".bright_yellow());
                    continue;
                };
                if task.is_empty() {
                    println!("{} Usage: /swarm <task>", "ℹ".bright_yellow());
                } else {
                    match self.run_swarm_with_queue(task).await {
                        Ok(_) => {}
                        Err(e) => println!("{} Swarm error: {}", "❌".bright_red(), e),
                    }
                }
                continue;
            }

            if input == "/queue" {
                println!("{} Usage:", "📨".bright_cyan());
                println!("  /queue <message>  — Enqueue a message");
                println!("  /queue list       — Show queued messages");
                println!("  /queue clear      — Clear all queued messages");
                println!("  /queue drop <n>   — Remove message by index");
                println!("  {} pending message(s)", self.pending_messages.len());
                continue;
            }

            if input == "/queue list" {
                let msgs = &self.pending_messages;
                if msgs.is_empty() {
                    println!("{} Queue is empty.", "📋".bright_cyan());
                } else {
                    println!("{} Queued messages ({}):", "📋".bright_cyan(), msgs.len());
                    for (i, msg) in msgs.iter().enumerate() {
                        let preview = preview_with_ellipsis(&msg.content, QUEUE_LIST_PREVIEW_BYTES);
                        println!("  {}. {}", i + 1, preview,);
                    }
                }
                continue;
            }

            if input == "/queue clear" {
                let count = self.pending_messages.len();
                self.pending_messages.clear();
                println!(
                    "{} Cleared {} queued message(s).",
                    "📋".bright_cyan(),
                    count
                );
                continue;
            }

            if let Some(idx_str) = input.strip_prefix("/queue drop ") {
                if let Ok(idx) = idx_str.trim().parse::<usize>() {
                    let idx = idx.saturating_sub(1); // 1-based to 0-based
                    if idx < self.pending_messages.len() {
                        let removed = self
                            .pending_messages
                            .remove(idx)
                            .expect("index checked above");
                        let preview =
                            preview_with_ellipsis(&removed.content, QUEUE_DROP_PREVIEW_BYTES);
                        println!(
                            "{} Removed message {}: {}",
                            "📋".bright_cyan(),
                            idx + 1,
                            preview,
                        );
                    } else {
                        println!(
                            "{} Invalid index. Use '/queue list' to see messages.",
                            "❌".bright_red()
                        );
                    }
                } else {
                    println!("{} Usage: /queue drop <number>", "ℹ".bright_yellow());
                }
                continue;
            }

            if input.starts_with("/queue ") {
                let Some(msg) = input.strip_prefix("/queue ").map(str::trim) else {
                    println!("{} Usage: /queue <message>", "ℹ".bright_yellow());
                    continue;
                };
                if msg.is_empty() {
                    println!("{} Usage: /queue <message>", "ℹ".bright_yellow());
                } else {
                    self.enqueue_pending_message(msg);
                    println!(
                        "{} Queued ({} pending)",
                        "📨".bright_green(),
                        self.pending_messages.len()
                    );
                }
                continue;
            }

            // /skills - list discovered user skills
            if input == "/skills" {
                if skill_registry.is_empty() {
                    println!("No skills found in ~/.selfware/skills or ./.selfware/skills");
                } else {
                    println!("Skills:");
                    for skill in skill_registry.list() {
                        println!("  /{:<12} - {}", skill.name, skill.description);
                    }
                }
                continue;
            }

            // /<skill-name> - inject a discovered user skill as instructions
            if let Some(name) = input.strip_prefix('/') {
                if let Some(skill) = skill_registry.get(name) {
                    self.messages.push(Message::system(format!(
                        "The user invoked the /{} skill. Follow these instructions:\n\n{}",
                        skill.name, skill.content
                    )));
                    println!(
                        "  Skill '{}' loaded — instructions added to context.",
                        skill.name
                    );
                    continue;
                }
            }

            // Unknown slash command — reject with a pointer to the command
            // list instead of forwarding it to the LLM as a paid chat
            // message. Absolute paths (/tmp/foo.rs) fall through to chat.
            if looks_like_slash_command(input) {
                reject_unknown_slash_command(input);
                continue;
            }

            // Display truncated preview and confirm for large pastes (interactive only)
            const LARGE_PASTE_THRESHOLD: usize = 3000;
            const PREVIEW_CHARS: usize = 200;

            if is_tty && input.len() > LARGE_PASTE_THRESHOLD {
                let lines: Vec<&str> = input.lines().collect();
                let line_count = lines.len();
                let char_count = input.len();

                let start_preview: String = input.chars().take(PREVIEW_CHARS).collect();
                let end_preview: String = input
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

                // Ask for confirmation before submitting large input
                print!("Submit this input? [Y/n] ");
                io::stdout().flush()?;
                let mut confirm = String::new();
                // Use block_in_place to prevent blocking the async runtime
                tokio::task::block_in_place(|| io::stdin().read_line(&mut confirm))?;
                let confirm = confirm.trim().to_lowercase();
                if confirm == "n" || confirm == "no" {
                    println!("Input cancelled.");
                    continue;
                }
            }

            match self.run_task_with_queue(input).await {
                Ok(_) => {}
                Err(e) => println!("{} Error: {}", "❌".bright_red(), e),
            }
        }

        // Auto-save conversation/session on exit so history isn't lost.
        if let Err(e) = self.save_checkpoint("interactive basic session exit") {
            warn!("Failed to auto-save session on exit: {}", e);
        }

        // Clean up any managed background processes before exiting
        crate::tools::process::cleanup_all_processes().await;

        Ok(())
    }

    /// Handle the `/scan <path>` command: build a RAG index for the given path.
    async fn handle_scan_command(&mut self, path_arg: &str) {
        use crate::analysis::vector_store::{EmbeddingBackend, TfIdfEmbeddingProvider};
        use crate::cognitive::rag::{RagConfig, RagEngine};

        let scan_path = std::path::Path::new(path_arg);
        let scan_path = if scan_path.is_relative() {
            std::env::current_dir().unwrap_or_default().join(scan_path)
        } else {
            scan_path.to_path_buf()
        };

        if !tokio::fs::try_exists(&scan_path).await.unwrap_or(false) {
            println!(
                "{} Path not found: {}",
                "x".bright_red(),
                scan_path.display()
            );
            return;
        }

        println!("{} Scanning {}...", ">>".bright_cyan(), scan_path.display());

        // Build config with priority-aware extensions
        let config = RagConfig {
            include_extensions: vec![
                // High priority (source code)
                "rs".into(),
                "toml".into(),
                "go".into(),
                "py".into(),
                "ts".into(),
                "js".into(),
                "java".into(),
                "c".into(),
                "cpp".into(),
                "h".into(),
                // Medium priority
                "json".into(),
                "yaml".into(),
                "yml".into(),
                "sql".into(),
                "sh".into(),
                // Low priority
                "md".into(),
                "txt".into(),
                "rst".into(),
                "html".into(),
                "css".into(),
            ],
            exclude_patterns: vec![
                "target/".into(),
                "node_modules/".into(),
                ".git/".into(),
                "__pycache__/".into(),
                "*.min.js".into(),
                "*.min.css".into(),
                "vendor/".into(),
                "dist/".into(),
                "build/".into(),
                "*.lock".into(),
            ],
            ..Default::default()
        };

        let provider = if let Some(profile) = self.config.resolve_model(Some("embedding")) {
            // Use HTTP embedding backend from [models.embedding] config
            let dim = profile.context_length.min(4096); // context_length doubles as dimension hint
                                                        // Profile key wins; fall back to the top-level api_key (e.g. the
                                                        // same OpenRouter key usually serves both chat and embeddings).
            let api_key = profile
                .api_key
                .as_ref()
                .or(self.config.api_key.as_ref())
                .map(|k| k.expose().to_string());
            Arc::new(EmbeddingBackend::Http(
                crate::analysis::vector_store::HttpEmbeddingProvider::new(
                    &profile.endpoint,
                    &profile.model,
                    if dim > 0 && dim <= 4096 { dim } else { 768 },
                )
                .with_api_key(api_key),
            ))
        } else {
            Arc::new(EmbeddingBackend::TfIdf(TfIdfEmbeddingProvider::default()))
        };
        let mut engine = RagEngine::new(&scan_path, provider, config);

        match engine.build_index().await {
            Ok(stats) => {
                // Compute source vs docs split
                let source_exts: std::collections::HashSet<&str> = [
                    "rs", "toml", "go", "py", "ts", "js", "java", "c", "cpp", "h",
                ]
                .iter()
                .copied()
                .collect();
                let source_files: usize = stats
                    .files_by_language
                    .iter()
                    .filter(|(k, _)| source_exts.contains(k.as_str()))
                    .map(|(_, v)| v)
                    .sum();
                let doc_files = stats.total_files.saturating_sub(source_files);

                println!(
                    "{} Indexed {} files ({} source, {} docs), {} chunks, ~{}K tokens in {}ms",
                    "ok".bright_green(),
                    stats.total_files,
                    source_files,
                    doc_files,
                    stats.total_chunks,
                    stats.total_tokens / 1000,
                    stats.build_time_ms,
                );

                // Store engine on agent for retrieval
                self.rag_engine = Some(Arc::new(tokio::sync::RwLock::new(engine)));
            }
            Err(e) => {
                println!("{} Scan failed: {}", "x".bright_red(), e);
            }
        }
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/agent/interactive/interactive_test.rs"]
mod tests;
