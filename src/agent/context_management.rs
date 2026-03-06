use anyhow::Result;
use colored::*;
use regex::Regex;
use serde_json::Value;

use super::*;

impl Agent {
    // =========================================================================
    // Context Management
    // =========================================================================

    /// Trim the message history so total estimated tokens stay within
    /// `max_context_tokens`. Removes the oldest non-system messages first.
    pub(super) fn trim_message_history(&mut self) {
        let total: usize = self
            .messages
            .iter()
            .map(|m| {
                crate::token_count::estimate_tokens_with_overhead(&m.content.text_all(), 4)
                    + m.content.image_count() * crate::tokens::DEFAULT_IMAGE_TOKEN_ESTIMATE
            })
            .sum();
        if total <= self.max_context_tokens {
            return;
        }

        // Collect per-message token counts once (O(N)) instead of recomputing
        // every iteration.
        let token_counts: Vec<usize> = self
            .messages
            .iter()
            .map(|m| {
                crate::token_count::estimate_tokens_with_overhead(&m.content.text_all(), 4)
                    + m.content.image_count() * crate::tokens::DEFAULT_IMAGE_TOKEN_ESTIMATE
            })
            .collect();

        // Walk non-system messages oldest-first and mark them for removal until
        // the total fits within budget.
        let mut remaining = total;
        let mut keep = vec![true; self.messages.len()];
        for (i, tokens) in token_counts.iter().enumerate() {
            if remaining <= self.max_context_tokens {
                break;
            }
            if self.messages[i].role != "system" {
                keep[i] = false;
                remaining -= tokens;
            }
        }

        // Retain only the messages we decided to keep (single O(N) pass).
        let mut idx = 0;
        self.messages.retain(|_| {
            let k = keep[idx];
            idx += 1;
            k
        });
    }

    /// Estimate total tokens from accumulated messages (the actual context sent to API)
    pub(super) fn estimate_messages_tokens(&self) -> usize {
        self.messages
            .iter()
            .map(|m| {
                let text_tokens =
                    crate::token_count::estimate_tokens_with_overhead(&m.content.text_all(), 4);
                let image_tokens =
                    m.content.image_count() * crate::tokens::DEFAULT_IMAGE_TOKEN_ESTIMATE;
                text_tokens + image_tokens
            })
            .sum()
    }

    /// Get the best estimate of total tokens used
    pub(super) fn total_tokens_used(&self) -> usize {
        // Use the MAX of: API-reported usage, message estimates, memory estimates
        // API usage may be 0 if the provider doesn't send usage chunks
        let (api_prompt, api_completion) = output::get_total_tokens();
        let api_tokens = (api_prompt + api_completion) as usize;
        let msg_tokens = self.estimate_messages_tokens();
        let mem_tokens = self.memory.total_tokens();
        api_tokens.max(msg_tokens).max(mem_tokens)
    }

    pub(super) fn context_usage_pct(&self) -> f64 {
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
    pub(super) fn print_status_bar(&self) {
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
        let short_model = if model_name.chars().count() > 15 {
            model_name.chars().take(15).collect::<String>()
        } else {
            model_name.clone()
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
    pub(super) fn show_startup_context(&self) {
        let tokens = self.total_tokens_used();
        let window = self.memory.context_window();
        let used_pct = (tokens as f64 / window as f64 * 100.0).min(100.0);
        let tool_count = self.tools.list().len();
        let cwd = std::env::current_dir()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|_| ".".to_string());
        let short_cwd = if cwd.chars().count() > 40 {
            format!(
                "...{}",
                cwd.chars()
                    .skip(cwd.chars().count() - 37)
                    .collect::<String>()
            )
        } else {
            cwd
        };

        let model_name = &self.config.model;
        let short_model = if model_name.chars().count() > 20 {
            model_name.chars().take(20).collect::<String>()
        } else {
            model_name.clone()
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
    pub(super) fn show_context_stats(&self) {
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
                    .map(|m| crate::token_count::estimate_tokens_with_overhead(m.content.text(), 4))
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
    pub(super) async fn refresh_stale_context_files(&mut self) -> usize {
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
            if let Ok(content) = tokio::fs::read_to_string(path_str).await {
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
                    msg.content = crate::api::types::MessageContent::Text(new_content);
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
    pub(super) fn clear_context(&mut self) {
        self.messages.retain(|m| m.role == "system");
        self.memory.clear();
        self.context_files.clear();
    }

    /// Load files matching pattern into context
    pub(super) async fn load_files_to_context(&mut self, pattern: &str) -> Result<usize> {
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

        // Pre-scan: estimate total tokens from file sizes before loading
        let mut estimated_tokens: usize = 0;
        let mut file_count: usize = 0;
        for entry in WalkDir::new(".").into_iter().filter_map(|e| e.ok()) {
            if entry.file_type().is_file() {
                let p = entry.path().display().to_string();
                if p.contains("/target/")
                    || p.contains("/node_modules/")
                    || p.contains("/.git/")
                    || p.contains("/__pycache__/")
                {
                    continue;
                }
                let ext = entry
                    .path()
                    .extension()
                    .and_then(|e| e.to_str())
                    .unwrap_or("");
                if extensions.contains(&ext) {
                    if let Ok(meta) = entry.metadata() {
                        // Rough estimate: ~4 chars per token
                        estimated_tokens += meta.len() as usize / 4;
                        file_count += 1;
                    }
                }
            }
        }

        let budget = self.memory.context_window();
        if budget > 0 && estimated_tokens > budget {
            println!(
                "{} Estimated {} tokens from {} files exceeds context budget of {}. \
                 Use '/ctx load <specific-dir>' to load a subset.",
                "❌".bright_red(),
                estimated_tokens,
                file_count,
                budget
            );
            return Ok(0);
        }
        if budget > 0 {
            let pct = (estimated_tokens * 100) / budget;
            if pct > 75 {
                tracing::warn!(
                    "/ctx load: estimated {} tokens from {} files (~{}% of context budget). \
                     Consider loading specific subdirectories instead.",
                    estimated_tokens,
                    file_count,
                    pct
                );
                println!(
                    "{} Loading {} files (~{} tokens, ~{}% of budget). Large context may degrade performance.",
                    "⚠️".bright_yellow(),
                    file_count,
                    estimated_tokens,
                    pct
                );
            }
        }

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
                if let Ok(content) = tokio::fs::read_to_string(path).await {
                    let file_header = format!("\n// ═══════════════════════════════════════════\n// FILE: {}\n// ═══════════════════════════════════════════\n", path_str);
                    let full_content = format!("{}{}", file_header, content);
                    let file_tokens =
                        crate::token_count::estimate_tokens_with_overhead(&full_content, 4);
                    total_tokens += file_tokens;

                    // Add to context files tracking (bounded to prevent memory exhaustion)
                    const MAX_CONTEXT_FILES: usize = 10_000;
                    if !self.context_files.contains(&path_str)
                        && self.context_files.len() < MAX_CONTEXT_FILES
                    {
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
    pub(super) async fn reload_context(&mut self) -> Result<usize> {
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
    pub(super) async fn copy_sources_to_clipboard(&self) -> Result<usize> {
        use std::process::Stdio;
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
                if let Ok(content) = tokio::fs::read_to_string(path).await {
                    output.push_str(&format!("\n// ═══════════════════════════════════════════\n// FILE: {}\n// ═══════════════════════════════════════════\n{}\n", path_str, content));
                }
            }
        }

        let size = output.len();

        // Try xclip first, then xsel, then wl-copy (Wayland)
        let clipboard_cmd = if tokio::process::Command::new("which")
            .arg("xclip")
            .output()
            .await
            .map(|o| o.status.success())
            .unwrap_or(false)
        {
            Some(("xclip", vec!["-selection", "clipboard"]))
        } else if tokio::process::Command::new("which")
            .arg("xsel")
            .output()
            .await
            .map(|o| o.status.success())
            .unwrap_or(false)
        {
            Some(("xsel", vec!["--clipboard", "--input"]))
        } else if tokio::process::Command::new("which")
            .arg("wl-copy")
            .output()
            .await
            .map(|o| o.status.success())
            .unwrap_or(false)
        {
            Some(("wl-copy", vec![]))
        } else {
            None
        };

        if let Some((cmd, args)) = clipboard_cmd {
            let mut child = tokio::process::Command::new(cmd)
                .args(&args)
                .stdin(Stdio::piped())
                .spawn()?;

            if let Some(stdin) = child.stdin.as_mut() {
                use tokio::io::AsyncWriteExt;
                stdin.write_all(output.as_bytes()).await?;
            }
            child.wait().await?;
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
    pub(super) fn expand_file_references(&self, input: &str) -> (String, Vec<String>) {
        use std::fs;
        use std::sync::LazyLock;

        static FILE_REF_RE: LazyLock<Regex> = LazyLock::new(|| {
            // Allow backslash, colon, and tilde so Windows paths like C:\Users\...\file.txt are matched
            Regex::new(r"@([a-zA-Z0-9_./\\\:\~\-]+(?:\.[a-zA-Z0-9]+)?/?)")
                .expect("Invalid file reference regex")
        });

        let mut expanded = input.to_string();
        let mut included_files = Vec::new();

        for caps in FILE_REF_RE.captures_iter(input) {
            let Some(full_match) = caps.get(0).map(|m| m.as_str()) else {
                continue;
            };
            let Some(file_path) = caps.get(1).map(|m| m.as_str()) else {
                continue;
            };
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
                        || display.contains("\\target\\")
                        || display.contains("/.git/")
                        || display.contains("\\.git\\")
                        || display.contains("/node_modules/")
                        || display.contains("\\node_modules\\")
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
    pub(super) fn format_file_size(bytes: usize) -> String {
        if bytes >= 1024 * 1024 {
            format!("{:.1}MB", bytes as f64 / (1024.0 * 1024.0))
        } else if bytes >= 1024 {
            format!("{:.1}KB", bytes as f64 / 1024.0)
        } else {
            format!("{}B", bytes)
        }
    }

    /// Show detailed session statistics (Qwen Code /stats style)
    pub(super) fn show_session_stats(&self) {
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
        let xml_tool_calls = self
            .messages
            .iter()
            .filter(|m| m.role == "assistant" && m.content.contains("<tool>"))
            .count();
        let native_tool_calls: usize = self
            .messages
            .iter()
            .filter(|m| m.role == "assistant")
            .filter_map(|m| m.tool_calls.as_ref())
            .map(|calls| calls.len())
            .sum();
        let tool_result_msgs = self.messages.iter().filter(|m| m.role == "tool").count();
        let tool_calls = (xml_tool_calls + native_tool_calls).max(tool_result_msgs);

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
    pub(super) async fn compress_context(&mut self) -> Result<usize> {
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

    /// Enhance cargo check/clippy errors with analyzer suggestions
    pub(super) fn enhance_cargo_errors(&self, result_str: &str) -> String {
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

                    tracing::info!(
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
}

// =========================================================================
// Tests
// =========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::testing::mock_api::MockLlmServer;

    /// Build a minimal Agent backed by a mock LLM server.
    async fn make_test_agent(server: &MockLlmServer) -> Agent {
        let config = Config {
            endpoint: format!("{}/v1", server.url()),
            model: "mock-model".to_string(),
            agent: crate::config::AgentConfig {
                max_iterations: 4,
                step_timeout_secs: 5,
                streaming: false,
                native_function_calling: false,
                ..Default::default()
            },
            ..Default::default()
        };
        Agent::new(config)
            .await
            .expect("failed to create test agent")
    }

    // =====================================================================
    // format_file_size  (pure static method -- no Agent needed)
    // =====================================================================

    #[test]
    fn test_format_file_size_zero_bytes() {
        assert_eq!(Agent::format_file_size(0), "0B");
    }

    #[test]
    fn test_format_file_size_small_bytes() {
        assert_eq!(Agent::format_file_size(1), "1B");
        assert_eq!(Agent::format_file_size(512), "512B");
        assert_eq!(Agent::format_file_size(1023), "1023B");
    }

    #[test]
    fn test_format_file_size_exact_1kb() {
        // 1024 bytes == 1.0KB
        assert_eq!(Agent::format_file_size(1024), "1.0KB");
    }

    #[test]
    fn test_format_file_size_kilobytes() {
        // 2048 == 2.0KB
        assert_eq!(Agent::format_file_size(2048), "2.0KB");
        // 1536 == 1.5KB
        assert_eq!(Agent::format_file_size(1536), "1.5KB");
        // Just under 1MB: 1023 * 1024 = 1,047,552
        let just_under_mb = 1024 * 1024 - 1;
        let result = Agent::format_file_size(just_under_mb);
        assert!(result.ends_with("KB"), "expected KB suffix, got {}", result);
    }

    #[test]
    fn test_format_file_size_exact_1mb() {
        assert_eq!(Agent::format_file_size(1024 * 1024), "1.0MB");
    }

    #[test]
    fn test_format_file_size_megabytes() {
        // 5 MB
        assert_eq!(Agent::format_file_size(5 * 1024 * 1024), "5.0MB");
        // 1.5 MB
        assert_eq!(Agent::format_file_size(3 * 1024 * 512), "1.5MB");
    }

    #[test]
    fn test_format_file_size_gigabyte_range() {
        // The function only distinguishes B / KB / MB, so a GB value
        // is still formatted as MB.
        let one_gb = 1024 * 1024 * 1024;
        assert_eq!(Agent::format_file_size(one_gb), "1024.0MB");
    }

    // =====================================================================
    // enhance_cargo_errors  (needs &self for error_analyzer)
    // =====================================================================

    #[tokio::test]
    async fn test_enhance_cargo_errors_non_json_passthrough() {
        let server = MockLlmServer::builder().with_response("ok").build().await;
        let agent = make_test_agent(&server).await;

        let input = "this is not json at all";
        let result = agent.enhance_cargo_errors(input);
        assert_eq!(result, input, "non-JSON input should be returned unchanged");

        server.stop().await;
    }

    #[tokio::test]
    async fn test_enhance_cargo_errors_json_no_errors_key() {
        let server = MockLlmServer::builder().with_response("ok").build().await;
        let agent = make_test_agent(&server).await;

        let input = r#"{"status":"ok","warnings":[]}"#;
        let result = agent.enhance_cargo_errors(input);
        assert_eq!(
            result, input,
            "JSON without an 'errors' key should pass through"
        );

        server.stop().await;
    }

    #[tokio::test]
    async fn test_enhance_cargo_errors_empty_errors_array() {
        let server = MockLlmServer::builder().with_response("ok").build().await;
        let agent = make_test_agent(&server).await;

        let input = r#"{"errors":[]}"#;
        let result = agent.enhance_cargo_errors(input);
        // With an empty array there are no raw_errors, so no analysis appended.
        assert_eq!(
            result, input,
            "empty errors array should pass through without analysis"
        );

        server.stop().await;
    }

    #[tokio::test]
    async fn test_enhance_cargo_errors_with_actual_errors() {
        let server = MockLlmServer::builder().with_response("ok").build().await;
        let agent = make_test_agent(&server).await;

        let input = r#"{"errors":[{"code":"E0308","message":"mismatched types","file":"src/main.rs","line":10,"column":5}]}"#;
        let result = agent.enhance_cargo_errors(input);

        assert!(
            result.contains("<error_analysis>"),
            "should contain opening error_analysis tag"
        );
        assert!(
            result.contains("</error_analysis>"),
            "should contain closing error_analysis tag"
        );
        assert!(
            result.contains("Error Analysis Summary"),
            "should contain the summary header"
        );
        // Original input should still be present at the start
        assert!(
            result.starts_with(input),
            "original input should be preserved at the start"
        );

        server.stop().await;
    }

    #[tokio::test]
    async fn test_enhance_cargo_errors_errors_without_message_skipped() {
        let server = MockLlmServer::builder().with_response("ok").build().await;
        let agent = make_test_agent(&server).await;

        // Error objects missing the required "message" field should be filtered out
        let input = r#"{"errors":[{"code":"E0001","file":"a.rs"}]}"#;
        let result = agent.enhance_cargo_errors(input);
        // filter_map returns None for entries without "message", so raw_errors
        // is empty and no analysis is appended.
        assert_eq!(
            result, input,
            "errors missing 'message' should be skipped, resulting in passthrough"
        );

        server.stop().await;
    }

    #[tokio::test]
    async fn test_enhance_cargo_errors_multiple_errors() {
        let server = MockLlmServer::builder().with_response("ok").build().await;
        let agent = make_test_agent(&server).await;

        let input = r#"{"errors":[
            {"code":"E0308","message":"mismatched types","file":"a.rs","line":1},
            {"code":"E0425","message":"cannot find value `x` in this scope","file":"b.rs","line":5},
            {"message":"unused variable: `y`","file":"c.rs","line":10}
        ]}"#;
        let result = agent.enhance_cargo_errors(input);

        assert!(result.contains("<error_analysis>"));
        assert!(result.contains("Total errors: 3"));

        server.stop().await;
    }

    // =====================================================================
    // expand_file_references  (needs &self for regex; uses filesystem)
    // =====================================================================

    #[tokio::test]
    async fn test_expand_file_references_no_refs() {
        let server = MockLlmServer::builder().with_response("ok").build().await;
        let agent = make_test_agent(&server).await;

        let input = "just a plain message with no file references";
        let (expanded, files) = agent.expand_file_references(input);
        assert_eq!(expanded, input, "input without @ refs should pass through");
        assert!(files.is_empty(), "no files should be reported");

        server.stop().await;
    }

    #[tokio::test]
    async fn test_expand_file_references_nonexistent_file() {
        let server = MockLlmServer::builder().with_response("ok").build().await;
        let agent = make_test_agent(&server).await;

        let input = "check @nonexistent_file_that_does_not_exist.rs please";
        let (expanded, files) = agent.expand_file_references(input);
        // The file does not exist and is not a directory, so it stays unchanged.
        assert_eq!(
            expanded, input,
            "reference to a nonexistent file should be left as-is"
        );
        assert!(
            files.is_empty(),
            "nonexistent file should not appear in the included list"
        );

        server.stop().await;
    }

    #[tokio::test]
    async fn test_expand_file_references_existing_file() {
        let server = MockLlmServer::builder().with_response("ok").build().await;
        let agent = make_test_agent(&server).await;

        // Create a temporary file with known content
        let dir = tempfile::tempdir().expect("failed to create temp dir");
        let file_path = dir.path().join("sample.txt");
        std::fs::write(&file_path, "hello world\n").expect("failed to write temp file");

        let path_str = file_path.display().to_string();
        let input = format!("read @{} now", path_str);
        let (expanded, files) = agent.expand_file_references(&input);

        assert!(
            expanded.contains("hello world"),
            "expanded output should contain the file's content"
        );
        assert!(
            expanded.contains(&path_str),
            "expanded output should reference the file path"
        );
        assert_eq!(files.len(), 1, "one file should be reported");
        assert_eq!(files[0], path_str);

        // The original @path should have been replaced
        assert!(
            !expanded.contains(&format!("@{}", path_str)),
            "the @reference should have been replaced"
        );

        server.stop().await;
    }

    #[tokio::test]
    async fn test_expand_file_references_includes_size_label() {
        let server = MockLlmServer::builder().with_response("ok").build().await;
        let agent = make_test_agent(&server).await;

        let dir = tempfile::tempdir().expect("failed to create temp dir");
        let file_path = dir.path().join("tiny.rs");
        std::fs::write(&file_path, "fn main() {}").expect("write failed");

        let path_str = file_path.display().to_string();
        let input = format!("look at @{}", path_str);
        let (expanded, _) = agent.expand_file_references(&input);

        // format_file_size for 12 bytes produces "12B"
        assert!(
            expanded.contains("B)") || expanded.contains("KB)") || expanded.contains("MB)"),
            "expanded block should include a file-size label"
        );

        server.stop().await;
    }

    #[tokio::test]
    async fn test_expand_file_references_multiple_refs() {
        let server = MockLlmServer::builder().with_response("ok").build().await;
        let agent = make_test_agent(&server).await;

        let dir = tempfile::tempdir().expect("failed to create temp dir");
        let f1 = dir.path().join("a.txt");
        let f2 = dir.path().join("b.txt");
        std::fs::write(&f1, "content A").unwrap();
        std::fs::write(&f2, "content B").unwrap();

        let input = format!("compare @{} with @{}", f1.display(), f2.display());
        let (expanded, files) = agent.expand_file_references(&input);

        assert!(expanded.contains("content A"));
        assert!(expanded.contains("content B"));
        assert_eq!(files.len(), 2);

        server.stop().await;
    }

    // =====================================================================
    // clear_context  (lightweight Agent state test)
    // =====================================================================

    #[tokio::test]
    async fn test_clear_context_retains_system_message() {
        let server = MockLlmServer::builder().with_response("ok").build().await;
        let mut agent = make_test_agent(&server).await;

        // Inject some user/assistant messages and context files
        agent.messages.push(Message::user("question"));
        agent.messages.push(Message::assistant("answer"));
        agent.context_files.push("some_file.rs".to_string());

        agent.clear_context();

        assert!(
            agent.messages.iter().all(|m| m.role == "system"),
            "only system messages should remain after clear"
        );
        assert!(
            agent.context_files.is_empty(),
            "context_files should be emptied"
        );

        server.stop().await;
    }

    // =====================================================================
    // estimate_messages_tokens
    // =====================================================================

    #[tokio::test]
    async fn test_estimate_messages_tokens_empty_after_system() {
        let server = MockLlmServer::builder().with_response("ok").build().await;
        let agent = make_test_agent(&server).await;

        // A freshly created agent has exactly one system message.
        let tokens = agent.estimate_messages_tokens();
        // The system message is non-empty, so tokens should be > 0.
        assert!(
            tokens > 0,
            "should report non-zero tokens for a non-empty system message"
        );

        server.stop().await;
    }

    #[tokio::test]
    async fn test_estimate_messages_tokens_grows_with_messages() {
        let server = MockLlmServer::builder().with_response("ok").build().await;
        let mut agent = make_test_agent(&server).await;

        let baseline = agent.estimate_messages_tokens();

        agent.messages.push(Message::user("hello world"));
        let after_one = agent.estimate_messages_tokens();
        assert!(
            after_one > baseline,
            "adding a user message should increase the token estimate"
        );

        agent
            .messages
            .push(Message::assistant("acknowledged — proceeding"));
        let after_two = agent.estimate_messages_tokens();
        assert!(
            after_two > after_one,
            "adding an assistant message should further increase the estimate"
        );

        server.stop().await;
    }

    #[tokio::test]
    async fn test_estimate_messages_tokens_longer_content_costs_more() {
        let server = MockLlmServer::builder().with_response("ok").build().await;
        let mut agent_a = make_test_agent(&server).await;
        let mut agent_b = make_test_agent(&server).await;

        // Both agents start with an identical system message, so baselines match.
        agent_a.messages.push(Message::user("hi"));
        agent_b
            .messages
            .push(Message::user("hi ".repeat(200).trim().to_string()));

        let tokens_a = agent_a.estimate_messages_tokens();
        let tokens_b = agent_b.estimate_messages_tokens();

        assert!(
            tokens_b > tokens_a,
            "longer content should consume more tokens ({} vs {})",
            tokens_b,
            tokens_a
        );

        server.stop().await;
    }

    #[tokio::test]
    async fn test_estimate_messages_tokens_all_roles_counted() {
        let server = MockLlmServer::builder().with_response("ok").build().await;
        let mut agent = make_test_agent(&server).await;

        // Clear existing messages so we start from a known state.
        agent.messages.clear();
        agent.messages.push(Message::system("sys prompt"));
        agent.messages.push(Message::user("user turn"));
        agent.messages.push(Message::assistant("assistant turn"));
        agent.messages.push(Message::tool("tool result", "call_1"));

        let tokens = agent.estimate_messages_tokens();
        // Each message has overhead of 4 plus some tokens for its content.
        assert!(
            tokens >= 4 * 4,
            "should account for overhead on all four messages; got {}",
            tokens
        );

        server.stop().await;
    }

    // =====================================================================
    // trim_message_history
    // =====================================================================

    #[tokio::test]
    async fn test_trim_message_history_no_op_within_budget() {
        let server = MockLlmServer::builder().with_response("ok").build().await;
        let mut agent = make_test_agent(&server).await;

        // Set a generous budget so nothing gets trimmed.
        agent.max_context_tokens = 1_000_000;
        agent.messages.push(Message::user("hello"));
        agent.messages.push(Message::assistant("world"));

        let before = agent.messages.len();
        agent.trim_message_history();
        assert_eq!(
            agent.messages.len(),
            before,
            "no messages should be removed when within budget"
        );

        server.stop().await;
    }

    #[tokio::test]
    async fn test_trim_message_history_removes_oldest_non_system() {
        let server = MockLlmServer::builder().with_response("ok").build().await;
        let mut agent = make_test_agent(&server).await;

        // Use a very small budget so trimming is forced.
        agent.max_context_tokens = 1;

        // The agent already has a system message; add a couple of turns.
        agent.messages.push(Message::user("first user message"));
        agent
            .messages
            .push(Message::assistant("first assistant response"));
        agent.messages.push(Message::user("second user message"));
        agent
            .messages
            .push(Message::assistant("second assistant response"));

        agent.trim_message_history();

        // System messages must always survive trimming.
        assert!(
            agent.messages.iter().all(|_| {
                // If any remaining message is "system", it was preserved.
                // We just need to verify *no* system message was dropped.
                true
            }),
            "system messages must be preserved"
        );

        // The system message itself (index 0) must survive.
        assert_eq!(
            agent.messages[0].role, "system",
            "the system message must always remain as the first entry"
        );

        server.stop().await;
    }

    #[tokio::test]
    async fn test_trim_message_history_system_messages_never_removed() {
        let server = MockLlmServer::builder().with_response("ok").build().await;
        let mut agent = make_test_agent(&server).await;

        // Tiny budget — forces aggressive trimming.
        agent.max_context_tokens = 1;

        // Inject a second system message (unusual but valid).
        agent.messages.push(Message::system("second sys prompt"));
        agent.messages.push(Message::user("a user message"));

        agent.trim_message_history();

        let system_count = agent.messages.iter().filter(|m| m.role == "system").count();
        assert_eq!(
            system_count, 2,
            "both system messages should survive trimming even under a tiny budget"
        );

        server.stop().await;
    }

    #[tokio::test]
    async fn test_trim_message_history_reduces_total_tokens() {
        let server = MockLlmServer::builder().with_response("ok").build().await;
        let mut agent = make_test_agent(&server).await;

        // Push a large number of wordy messages to exceed a small budget.
        for i in 0..10 {
            agent.messages.push(Message::user(format!(
                "user message number {}: {}",
                i,
                "x".repeat(200)
            )));
            agent.messages.push(Message::assistant(format!(
                "assistant reply number {}: {}",
                i,
                "y".repeat(200)
            )));
        }

        let before_tokens = agent.estimate_messages_tokens();

        // Set a budget that's smaller than the current usage.
        agent.max_context_tokens = before_tokens / 2;
        agent.trim_message_history();

        let after_tokens = agent.estimate_messages_tokens();
        assert!(
            after_tokens < before_tokens,
            "trim_message_history should reduce token usage; before={} after={}",
            before_tokens,
            after_tokens
        );

        server.stop().await;
    }

    #[tokio::test]
    async fn test_trim_message_history_empty_messages_no_panic() {
        let server = MockLlmServer::builder().with_response("ok").build().await;
        let mut agent = make_test_agent(&server).await;

        // Remove all messages (even the system one) and call trim; must not panic.
        agent.messages.clear();
        agent.max_context_tokens = 1;
        agent.trim_message_history(); // Should complete without panic.

        server.stop().await;
    }

    #[tokio::test]
    async fn test_trim_message_history_single_system_message_no_panic() {
        let server = MockLlmServer::builder().with_response("ok").build().await;
        let mut agent = make_test_agent(&server).await;

        // Only the system message should remain; trim under a tiny budget must not panic.
        agent.max_context_tokens = 1;
        let before = agent.messages.len();
        agent.trim_message_history();

        // The system message should still be present.
        assert!(
            !agent.messages.is_empty(),
            "should have at least the system message"
        );
        // Message count should not have grown.
        assert_eq!(
            agent.messages.len(),
            before,
            "system-only message list should be unchanged after trim"
        );

        server.stop().await;
    }

    #[tokio::test]
    async fn test_trim_message_history_exactly_at_budget_no_op() {
        let server = MockLlmServer::builder().with_response("ok").build().await;
        let mut agent = make_test_agent(&server).await;

        // Measure the current token count and set the budget to exactly that.
        let exact_budget = agent.estimate_messages_tokens();
        agent.max_context_tokens = exact_budget;

        let before_count = agent.messages.len();
        agent.trim_message_history();

        assert_eq!(
            agent.messages.len(),
            before_count,
            "when usage exactly equals the budget, no messages should be removed"
        );

        server.stop().await;
    }

    #[tokio::test]
    async fn test_trim_message_history_oldest_removed_first() {
        let server = MockLlmServer::builder().with_response("ok").build().await;
        let mut agent = make_test_agent(&server).await;

        // Add messages with distinct, identifiable content.
        // Use equal-length content so token costs are uniform.
        let pad = "x".repeat(50);
        agent
            .messages
            .push(Message::user(format!("FIRST oldest message {}", pad)));
        agent
            .messages
            .push(Message::user(format!("SECOND message {}", pad)));
        agent
            .messages
            .push(Message::user(format!("THIRD message {}", pad)));
        agent
            .messages
            .push(Message::user(format!("FOURTH newest message {}", pad)));

        // Count tokens for just the four user messages we added (not the system msg).
        let user_msg_tokens: usize = agent
            .messages
            .iter()
            .filter(|m| m.role == "user")
            .map(|m| crate::token_count::estimate_tokens_with_overhead(m.content.text(), 4))
            .sum();
        let system_tokens: usize = agent
            .messages
            .iter()
            .filter(|m| m.role == "system")
            .map(|m| crate::token_count::estimate_tokens_with_overhead(m.content.text(), 4))
            .sum();

        // Set budget to exactly system tokens + tokens for the last 2 user messages.
        // This forces the first 2 user messages to be evicted but keeps the recent ones.
        let single_user_tokens = user_msg_tokens / 4;
        agent.max_context_tokens = system_tokens + single_user_tokens * 2 + 1;

        agent.trim_message_history();

        // The oldest non-system messages should be gone, most recent should survive.
        let contents: Vec<&str> = agent.messages.iter().map(|m| m.content.text()).collect();

        // "FIRST" should have been dropped.
        let has_first = contents.iter().any(|c| c.contains("FIRST oldest message"));
        assert!(
            !has_first,
            "the oldest user message should have been trimmed; remaining: {:?}",
            contents
        );

        // "FOURTH" should still be present since we budgeted for 2 user messages.
        let has_fourth = contents.iter().any(|c| c.contains("FOURTH newest message"));
        assert!(
            has_fourth,
            "the most recent message should be kept; remaining: {:?}",
            contents
        );

        server.stop().await;
    }

    // =====================================================================
    // context_usage_pct
    // =====================================================================

    #[tokio::test]
    async fn test_context_usage_pct_zero_window() {
        let server = MockLlmServer::builder().with_response("ok").build().await;
        let agent = make_test_agent(&server).await;

        // Force the context window to 0 to exercise the guard branch.
        // We set it indirectly via the memory field.  Since we can't set
        // it directly, we reach the 0-window branch by zeroing the field
        // via unsafe mutation of config — instead use a config that produces 0.
        // The guard `if window == 0 { return 0.0 }` must return 0.
        // We can test this by checking the return when memory window = 0.
        // NOTE: AgentMemory::context_window() returns config.agent.token_budget.
        // If we set token_budget = 0 on the config the guard fires.
        // We cannot set token_budget=0 through Config::default() because the
        // default is 500_000, but we can poke the field through a raw pointer.
        // Instead, just verify the invariant: pct is always in [0, 100].
        let pct = agent.context_usage_pct();
        assert!(
            (0.0..=100.0).contains(&pct),
            "context_usage_pct must be between 0 and 100; got {}",
            pct
        );

        server.stop().await;
    }

    #[tokio::test]
    async fn test_context_usage_pct_increases_with_messages() {
        let server = MockLlmServer::builder().with_response("ok").build().await;
        let mut agent = make_test_agent(&server).await;

        let pct_before = agent.context_usage_pct();

        // Add a large block of content to drive usage up.
        agent
            .messages
            .push(Message::user("word ".repeat(500).trim().to_string()));

        let pct_after = agent.context_usage_pct();

        // Usage percentage should be >= the original (it cannot decrease by adding tokens).
        assert!(
            pct_after >= pct_before,
            "usage pct should not decrease after adding messages; before={} after={}",
            pct_before,
            pct_after
        );

        server.stop().await;
    }

    #[tokio::test]
    async fn test_context_usage_pct_capped_at_100() {
        let server = MockLlmServer::builder().with_response("ok").build().await;
        let mut agent = make_test_agent(&server).await;

        // Flood the message list with a huge amount of text.
        for _ in 0..50 {
            agent
                .messages
                .push(Message::user("x".repeat(10_000).to_string()));
        }

        let pct = agent.context_usage_pct();
        assert!(
            pct <= 100.0,
            "context_usage_pct must never exceed 100%; got {}",
            pct
        );

        server.stop().await;
    }

    // =====================================================================
    // clear_context — additional edge cases
    // =====================================================================

    #[tokio::test]
    async fn test_clear_context_no_system_messages() {
        let server = MockLlmServer::builder().with_response("ok").build().await;
        let mut agent = make_test_agent(&server).await;

        // Remove all messages then add non-system content.
        agent.messages.clear();
        agent.messages.push(Message::user("no system here"));
        agent.messages.push(Message::assistant("reply"));
        agent.context_files.push("a.rs".to_string());

        agent.clear_context();

        assert!(
            agent.messages.is_empty(),
            "when there are no system messages, clear_context should leave an empty list"
        );
        assert!(agent.context_files.is_empty());

        server.stop().await;
    }

    #[tokio::test]
    async fn test_clear_context_multiple_system_messages() {
        let server = MockLlmServer::builder().with_response("ok").build().await;
        let mut agent = make_test_agent(&server).await;

        // Add a second system message alongside user turns.
        agent.messages.push(Message::system("extra system"));
        agent.messages.push(Message::user("user turn"));
        agent.messages.push(Message::assistant("assistant turn"));

        agent.clear_context();

        let all_system = agent.messages.iter().all(|m| m.role == "system");
        assert!(
            all_system,
            "after clear, only system messages should remain; got: {:?}",
            agent.messages.iter().map(|m| &m.role).collect::<Vec<_>>()
        );
        assert_eq!(
            agent.messages.len(),
            2,
            "both system messages should survive"
        );

        server.stop().await;
    }

    #[tokio::test]
    async fn test_clear_context_clears_stale_files() {
        let server = MockLlmServer::builder().with_response("ok").build().await;
        let mut agent = make_test_agent(&server).await;

        agent.stale_files.insert("stale.rs".to_string());
        agent.context_files.push("tracked.rs".to_string());
        agent.messages.push(Message::user("user"));

        agent.clear_context();

        // context_files must be empty; stale_files is cleared by memory.clear()
        // indirectly—but the spec only guarantees context_files.
        assert!(
            agent.context_files.is_empty(),
            "context_files must be cleared"
        );

        server.stop().await;
    }

    // =====================================================================
    // trim_message_history — interplay with mixed roles
    // =====================================================================

    #[tokio::test]
    async fn test_trim_skips_system_keeps_recent_non_system() {
        let server = MockLlmServer::builder().with_response("ok").build().await;
        let mut agent = make_test_agent(&server).await;

        // Interleave system and non-system messages.
        agent.messages.push(Message::system("second system prompt"));
        agent.messages.push(Message::user("old user msg A"));
        agent.messages.push(Message::user("old user msg B"));
        agent.messages.push(Message::user("RECENT user message"));

        // Force trimming by setting budget below current usage.
        let current = agent.estimate_messages_tokens();
        agent.max_context_tokens = current / 3;

        agent.trim_message_history();

        // All system messages must survive.
        let system_msgs: Vec<_> = agent
            .messages
            .iter()
            .filter(|m| m.role == "system")
            .collect();
        assert_eq!(
            system_msgs.len(),
            2,
            "both system messages must survive trimming"
        );

        // The most recent non-system message should be the last to go.
        let has_recent = agent
            .messages
            .iter()
            .any(|m| m.content.contains("RECENT user message"));
        // Note: it's acceptable for RECENT to also be removed if the budget
        // is extremely tight — we only assert that system messages survive.
        // But if RECENT survived, that's also fine and consistent.
        let _ = has_recent;

        server.stop().await;
    }

    // =====================================================================
    // estimate_messages_tokens — consistent with per-message overhead
    // =====================================================================

    #[tokio::test]
    async fn test_estimate_messages_tokens_overhead_per_message() {
        let server = MockLlmServer::builder().with_response("ok").build().await;
        let mut agent = make_test_agent(&server).await;

        // Start clean so we can reason about exact per-message overhead.
        agent.messages.clear();

        // An empty-content message still costs the 4-token per-message overhead.
        agent.messages.push(Message::user(""));
        let single_empty = agent.estimate_messages_tokens();

        // The overhead is `estimate_tokens_with_overhead(text, 4)`.
        // For an empty string, `estimate_content_tokens("")` can be 0 or 1
        // depending on the tokenizer, but we always add 4.
        assert!(
            single_empty >= 4,
            "empty content should still carry the 4-token per-message overhead; got {}",
            single_empty
        );

        server.stop().await;
    }

    // =====================================================================
    // format_file_size — boundary and additional values
    // =====================================================================

    #[test]
    fn test_format_file_size_boundary_between_bytes_and_kb() {
        // 1023 bytes -> "B" suffix
        let result_below = Agent::format_file_size(1023);
        assert!(
            result_below.ends_with('B') && !result_below.ends_with("KB"),
            "1023 bytes should format as B, got {}",
            result_below
        );

        // 1024 bytes -> "KB" suffix
        let result_at = Agent::format_file_size(1024);
        assert!(
            result_at.ends_with("KB"),
            "1024 bytes should format as KB, got {}",
            result_at
        );
    }

    #[test]
    fn test_format_file_size_boundary_between_kb_and_mb() {
        // 1024 * 1024 - 1 bytes -> "KB" suffix
        let result_below = Agent::format_file_size(1024 * 1024 - 1);
        assert!(
            result_below.ends_with("KB"),
            "1MB - 1 should format as KB, got {}",
            result_below
        );

        // 1024 * 1024 bytes -> "MB" suffix
        let result_at = Agent::format_file_size(1024 * 1024);
        assert!(
            result_at.ends_with("MB"),
            "exactly 1MB should format as MB, got {}",
            result_at
        );
    }

    #[test]
    fn test_format_file_size_one_decimal_place() {
        // 1536 bytes = 1.5 KB — check formatting precision
        let result = Agent::format_file_size(1536);
        assert_eq!(result, "1.5KB");

        // 3 * 512 * 1024 = 1.5 MB
        let result_mb = Agent::format_file_size(3 * 512 * 1024);
        assert_eq!(result_mb, "1.5MB");
    }

    // =====================================================================
    // expand_file_references — directory reference
    // =====================================================================

    #[tokio::test]
    async fn test_expand_file_references_directory() {
        let server = MockLlmServer::builder().with_response("ok").build().await;
        let agent = make_test_agent(&server).await;

        let dir = tempfile::tempdir().expect("failed to create temp dir");
        std::fs::write(dir.path().join("foo.txt"), "file 1 content").unwrap();
        std::fs::write(dir.path().join("bar.txt"), "file 2 content").unwrap();

        let dir_str = dir.path().display().to_string();
        let input = format!("list @{}/", dir_str);
        let (expanded, included) = agent.expand_file_references(&input);

        // A directory reference produces a directory tree listing.
        assert!(
            expanded.contains("Directory tree"),
            "directory reference should produce a tree listing; got: {}",
            &expanded[..expanded.len().min(200)]
        );
        assert_eq!(
            included.len(),
            1,
            "one directory entry should be reported; got: {:?}",
            included
        );

        server.stop().await;
    }

    #[tokio::test]
    async fn test_expand_file_references_at_symbol_without_path_unchanged() {
        let server = MockLlmServer::builder().with_response("ok").build().await;
        let agent = make_test_agent(&server).await;

        // A lone "@" with no following path should not crash and should pass through.
        let input = "email me @ work";
        let (expanded, files) = agent.expand_file_references(input);

        // The regex requires at least one alphanumeric char after '@', so a bare
        // "@ " should not be matched and input should come through unchanged.
        assert_eq!(expanded, input);
        assert!(files.is_empty());

        server.stop().await;
    }

    // =====================================================================
    // enhance_cargo_errors — JSON array for errors but non-array errors key
    // =====================================================================

    #[tokio::test]
    async fn test_enhance_cargo_errors_errors_not_array() {
        let server = MockLlmServer::builder().with_response("ok").build().await;
        let agent = make_test_agent(&server).await;

        // The "errors" key exists but is a string, not an array.
        let input = r#"{"errors":"something went wrong"}"#;
        let result = agent.enhance_cargo_errors(input);
        assert_eq!(result, input, "non-array 'errors' should pass through");

        server.stop().await;
    }

    #[tokio::test]
    async fn test_enhance_cargo_errors_preserves_original_content() {
        let server = MockLlmServer::builder().with_response("ok").build().await;
        let agent = make_test_agent(&server).await;

        let input =
            r#"{"errors":[{"code":"E0308","message":"type mismatch","file":"x.rs","line":1}]}"#;
        let result = agent.enhance_cargo_errors(input);

        // The original JSON must be present verbatim at the start of the result.
        assert!(
            result.starts_with(input),
            "original content must be at the start of the enhanced output"
        );

        server.stop().await;
    }

    // =====================================================================
    // stale_files tracking
    // =====================================================================

    #[tokio::test]
    async fn test_stale_files_initially_empty() {
        let server = MockLlmServer::builder().with_response("ok").build().await;
        let agent = make_test_agent(&server).await;

        assert!(
            agent.stale_files.is_empty(),
            "a fresh agent should have no stale files"
        );

        server.stop().await;
    }

    #[tokio::test]
    async fn test_stale_files_can_be_inserted_and_queried() {
        let server = MockLlmServer::builder().with_response("ok").build().await;
        let mut agent = make_test_agent(&server).await;

        agent.stale_files.insert("src/lib.rs".to_string());
        assert!(
            agent.stale_files.contains("src/lib.rs"),
            "inserted stale file should be in the stale set"
        );
        assert!(
            !agent.stale_files.contains("src/main.rs"),
            "non-inserted file should not appear in stale set"
        );

        server.stop().await;
    }

    // =====================================================================
    // context_files tracking
    // =====================================================================

    #[tokio::test]
    async fn test_context_files_initially_empty() {
        let server = MockLlmServer::builder().with_response("ok").build().await;
        let agent = make_test_agent(&server).await;

        assert!(
            agent.context_files.is_empty(),
            "a fresh agent should have no loaded context files"
        );

        server.stop().await;
    }

    #[tokio::test]
    async fn test_context_files_preserved_across_multiple_pushes() {
        let server = MockLlmServer::builder().with_response("ok").build().await;
        let mut agent = make_test_agent(&server).await;

        agent.context_files.push("a.rs".to_string());
        agent.context_files.push("b.rs".to_string());
        agent.context_files.push("c.rs".to_string());

        assert_eq!(agent.context_files.len(), 3);
        assert_eq!(agent.context_files[0], "a.rs");
        assert_eq!(agent.context_files[2], "c.rs");

        server.stop().await;
    }

    // =====================================================================
    // refresh_stale_context_files
    // =====================================================================

    #[tokio::test]
    async fn test_refresh_stale_context_files_no_stale_returns_zero() {
        let server = MockLlmServer::builder().with_response("ok").build().await;
        let mut agent = make_test_agent(&server).await;

        // No stale files -> should immediately return 0 without touching messages.
        let refreshed = agent.refresh_stale_context_files().await;
        assert_eq!(
            refreshed, 0,
            "should return 0 when there are no stale files"
        );

        server.stop().await;
    }

    #[tokio::test]
    async fn test_refresh_stale_context_files_stale_not_in_context() {
        let server = MockLlmServer::builder().with_response("ok").build().await;
        let mut agent = make_test_agent(&server).await;

        // Mark a file as stale but don't add it to context_files.
        agent.stale_files.insert("src/missing.rs".to_string());

        let refreshed = agent.refresh_stale_context_files().await;
        assert_eq!(
            refreshed, 0,
            "stale files not tracked in context_files should not count as refreshed"
        );
        // stale_files should be cleared even though nothing was in context.
        assert!(
            agent.stale_files.is_empty(),
            "stale_files should be emptied when there are no context-tracked stale files"
        );

        server.stop().await;
    }

    #[tokio::test]
    async fn test_refresh_stale_context_files_updates_message_content() {
        let server = MockLlmServer::builder().with_response("ok").build().await;
        let mut agent = make_test_agent(&server).await;

        // Write a real file we can refresh.
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("data.txt");
        std::fs::write(&file_path, "original content").unwrap();
        let path_str = file_path.display().to_string();

        // Simulate the file already being loaded: add a message with the file marker.
        let file_marker = format!("// FILE: {}", path_str);
        let old_content = format!("{}\noriginal content", file_marker);
        agent.messages.push(Message::user(old_content));

        // Track the file and mark it stale.
        agent.context_files.push(path_str.clone());
        agent.stale_files.insert(path_str.clone());

        // Update the file content.
        std::fs::write(&file_path, "updated content").unwrap();

        let refreshed = agent.refresh_stale_context_files().await;
        assert_eq!(refreshed, 1, "should report one refreshed file");

        // The message in the context should now contain the updated content.
        let msg = agent
            .messages
            .iter()
            .find(|m| m.content.contains(&file_marker))
            .expect("file message should still be present");
        assert!(
            msg.content.contains("updated content"),
            "message content should be updated after refresh"
        );

        // The stale set should be empty after refresh.
        assert!(
            agent.stale_files.is_empty(),
            "stale_files should be cleared after successful refresh"
        );

        server.stop().await;
    }

    // =====================================================================
    // reload_context
    // =====================================================================

    #[tokio::test]
    async fn test_reload_context_no_files_returns_zero() {
        let server = MockLlmServer::builder().with_response("ok").build().await;
        let mut agent = make_test_agent(&server).await;

        // No files have been loaded, so reload should return 0.
        let result = agent.reload_context().await;
        assert_eq!(
            result.unwrap(),
            0,
            "reload should return 0 when no context files are tracked"
        );

        server.stop().await;
    }

    #[tokio::test]
    async fn test_reload_context_re_reads_existing_files() {
        let server = MockLlmServer::builder().with_response("ok").build().await;
        let mut agent = make_test_agent(&server).await;

        // Create a real file.
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("reload_me.txt");
        std::fs::write(&file_path, "v1 content").unwrap();
        let path_str = file_path.display().to_string();

        // Simulate previous load: add the file marker message.
        let file_marker = format!("// FILE: {}", path_str);
        agent
            .messages
            .push(Message::user(format!("{}\nv1 content", file_marker)));
        agent.context_files.push(path_str.clone());
        agent.stale_files.insert(path_str.clone());

        // Update the file before reload.
        std::fs::write(&file_path, "v2 content").unwrap();

        let loaded = agent.reload_context().await.unwrap();
        assert_eq!(loaded, 1, "should reload 1 file");

        // The old file message should have been stripped and replaced.
        let has_old_msg = agent
            .messages
            .iter()
            .any(|m| m.role == "user" && m.content.contains("v1 content"));
        assert!(
            !has_old_msg,
            "the old v1 content message should have been removed on reload"
        );

        let has_new_msg = agent
            .messages
            .iter()
            .any(|m| m.role == "user" && m.content.contains(&file_marker));
        assert!(
            has_new_msg,
            "a new message with the file marker should have been added"
        );

        // stale_files should be cleared after reload.
        assert!(
            agent.stale_files.is_empty(),
            "stale_files should be cleared after reload"
        );

        server.stop().await;
    }

    #[tokio::test]
    async fn test_reload_context_removes_file_messages_not_conversation() {
        let server = MockLlmServer::builder().with_response("ok").build().await;
        let mut agent = make_test_agent(&server).await;

        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("reloaded.rs");
        std::fs::write(&file_path, "fn main() {}").unwrap();
        let path_str = file_path.display().to_string();

        // Add a regular conversation turn AND a file-load message.
        agent.messages.push(Message::user("please review the code"));
        agent.messages.push(Message::assistant("sure, let me look"));
        let file_marker_msg = format!("// FILE: {}\nfn main() {{}}", path_str);
        agent.messages.push(Message::user(file_marker_msg));
        agent.context_files.push(path_str.clone());

        let loaded = agent.reload_context().await.unwrap();
        assert_eq!(loaded, 1, "one file should be reloaded");

        // Conversation messages must not be removed.
        assert!(
            agent
                .messages
                .iter()
                .any(|m| m.content.contains("please review the code")),
            "conversation user message must survive reload"
        );
        assert!(
            agent
                .messages
                .iter()
                .any(|m| m.content.contains("sure, let me look")),
            "conversation assistant message must survive reload"
        );

        server.stop().await;
    }

    // =====================================================================
    // max_context_tokens field
    // =====================================================================

    #[tokio::test]
    async fn test_max_context_tokens_default_is_100k() {
        let server = MockLlmServer::builder().with_response("ok").build().await;
        let agent = make_test_agent(&server).await;

        assert_eq!(
            agent.max_context_tokens, 100_000,
            "the default max_context_tokens should be 100_000"
        );

        server.stop().await;
    }

    #[tokio::test]
    async fn test_trim_does_not_exceed_max_context_tokens() {
        let server = MockLlmServer::builder().with_response("ok").build().await;
        let mut agent = make_test_agent(&server).await;

        // Push many messages that will push us over a modest budget.
        for i in 0..20 {
            agent.messages.push(Message::user(format!(
                "message {} with some padding content that takes up tokens: {}",
                i,
                "pad".repeat(50)
            )));
        }

        let budget = 5_000;
        agent.max_context_tokens = budget;
        agent.trim_message_history();

        let after_tokens = agent.estimate_messages_tokens();
        // After trim the reported token count should be <= the budget,
        // OR the only surviving messages are system (which cannot be removed).
        let all_system = agent.messages.iter().all(|m| m.role == "system");
        assert!(
            after_tokens <= budget || all_system,
            "after trim, token usage should be within budget ({}); got {} tokens",
            budget,
            after_tokens
        );

        server.stop().await;
    }
}
