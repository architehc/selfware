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
            .map(|m| crate::token_count::estimate_tokens_with_overhead(&m.content, 4))
            .sum();
        if total <= self.max_context_tokens {
            return;
        }

        // Collect per-message token counts once (O(N)) instead of recomputing
        // every iteration.
        let token_counts: Vec<usize> = self
            .messages
            .iter()
            .map(|m| crate::token_count::estimate_tokens_with_overhead(&m.content, 4))
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
            .map(|m| crate::token_count::estimate_tokens_with_overhead(&m.content, 4))
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
            Regex::new(r"@([a-zA-Z0-9_./\-]+(?:\.[a-zA-Z0-9]+)?/?)")
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
