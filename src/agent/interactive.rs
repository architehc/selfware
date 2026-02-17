use anyhow::Result;
use colored::*;

use super::*;

impl Agent {
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
            "🦊 Selfware Interactive Mode".bright_cyan(),
            mode_indicator.bright_yellow()
        );
        self.show_startup_context();
        println!(
            "  Type {} for commands, {} for context, {} to quit",
            "/help".bright_cyan(),
            "/ctx".bright_cyan(),
            "exit".bright_cyan(),
        );

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
                    "│  {} /cost              Token usage & cost           │",
                    "💰".bright_white()
                );
                println!(
                    "│  {} /model             Model configuration          │",
                    "🤖".bright_white()
                );
                println!(
                    "│  {} /compact           Toggle compact mode          │",
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

            // === New slash commands ===

            if input == "/diff" {
                match std::process::Command::new("git")
                    .args(["diff", "--stat"])
                    .output()
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
                match std::process::Command::new("git")
                    .args(["status", "--short", "--branch"])
                    .output()
                {
                    Ok(out) => {
                        let stdout = String::from_utf8_lossy(&out.stdout);
                        println!("{}", stdout);
                    }
                    Err(e) => println!("{} git status failed: {}", "✗".bright_red(), e),
                }
                continue;
            }

            if input == "/undo" {
                if let Some(checkpoint) = self.edit_history.undo() {
                    let mut restored = 0;
                    for (path, snapshot) in &checkpoint.files {
                        if std::fs::write(path, &snapshot.content).is_ok() {
                            println!(
                                "  {} Restored {}",
                                "✓".bright_green(),
                                path.display().to_string().bright_white()
                            );
                            restored += 1;
                        }
                    }
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

            if input == "/compact" {
                let new_compact = !output::is_compact();
                output::init(
                    new_compact,
                    output::is_verbose(),
                    output::should_show_tokens(),
                );
                println!(
                    "{} Compact mode: {}",
                    "⚙".bright_cyan(),
                    if new_compact {
                        "ON".bright_green()
                    } else {
                        "OFF".bright_red()
                    }
                );
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
}
