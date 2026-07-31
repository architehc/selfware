use colored::*;

use super::*;

/// Output-token price per 1M tokens in USD for known models (OpenRouter
/// rates from the 2026-07 capability matrix). Unknown models fall back to
/// $3.00/1M (the old static estimate).
fn price_per_1m(model: &str) -> f64 {
    let m = model.to_ascii_lowercase();
    if m.contains("kimi-k3") {
        15.00
    } else if m.contains("kimi-k2") {
        2.40
    } else if m.contains("glm-5") {
        2.00
    } else if m.contains("gpt-4o-mini") {
        0.60
    } else if m.contains("gemma-4") {
        0.40
    } else if m.contains("deepseek") && m.contains("flash") {
        0.14
    } else if m.contains("qwen3.6") {
        2.40
    } else if m.contains("minimax") {
        1.40
    } else if m.contains("laguna") {
        0.50
    } else {
        3.00
    }
}

/// Rough USD cost for `tokens` output tokens on the given model.
pub fn estimated_cost_usd(model: &str, tokens: usize) -> f64 {
    tokens as f64 / 1_000_000.0 * price_per_1m(model)
}

impl Agent {
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

        // Cost estimate from model-aware pricing (per 1M output tokens,
        // OpenRouter rates as of the 2026-07 model matrix; falls back to a
        // generic $3/M for unknown models).
        let cost = estimated_cost_usd(&self.config.model, tokens);

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

        // Left side: mode + hint (+ trust-gate flag once it has sanitized anything)
        let trust_flag = if self.trust_gate_findings > 0 {
            format!(" trust:{}", self.trust_gate_findings)
        } else {
            String::new()
        };
        let left = format!("[{}] ? for shortcuts{}", mode, trust_flag);
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

        let trust_colored = if self.trust_gate_findings > 0 {
            format!(" trust:{}", self.trust_gate_findings).bright_yellow()
        } else {
            "".into()
        };

        println!(
            " {} {}{}{}  {} {:.1}% ({:.1}k/{:.0}k) {} [{}]",
            mode_colored,
            "? for shortcuts".dimmed(),
            trust_colored,
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
        let files_loaded = self.file_tracker.context_files.len();

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
        if !self.file_tracker.context_files.is_empty() {
            println!();
            println!("  {}📄 Context Files:{}", patina_light, reset);
            let mut total_file_tokens = 0usize;
            for path_str in &self.file_tracker.context_files {
                let file_tokens = self
                    .messages
                    .iter()
                    .find(|m| {
                        m.role == "user" && m.content.contains(&format!("// FILE: {}", path_str))
                    })
                    .map(|m| crate::token_count::estimate_tokens_with_overhead(m.content.text(), 4))
                    .unwrap_or(0);
                total_file_tokens += file_tokens;
                let is_stale = self.file_tracker.stale_files.contains(path_str);
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
                self.file_tracker.context_files.len(),
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

    /// Show detailed session statistics (Qwen Code /stats style)
    pub(super) async fn show_session_stats(&self) {
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
            self.file_tracker.context_files.len(),
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
        // Tool cache stats
        let tc_stats = self.cache_manager.tool_cache.stats().await;
        println!(
            "  {}│{}  {bold}{}◇ TOOL CACHE{}{:<44}    {}│{}",
            patina, reset, sand, reset, "", patina, reset
        );
        println!(
            "  {}│{}     Entries         {:>8} / {:<8}                          {}│{}",
            patina, reset, tc_stats.entries, tc_stats.max_entries, patina, reset
        );
        println!(
            "  {}│{}     TTL             {:>8}s                                   {}│{}",
            patina, reset, tc_stats.default_ttl_secs, patina, reset
        );
        println!(
            "  {}│{}                                                                    {}│{}",
            patina, reset, patina, reset
        );

        // Local-first coordinator stats
        let lf_stats = self.cache_manager.local_first.stats();
        println!(
            "  {}│{}  {bold}{}◆ LOCAL-FIRST{}{:<43}    {}│{}",
            patina, reset, sand, reset, "", patina, reset
        );
        println!(
            "  {}│{}     Cache Entries   {:>8}  (hit rate: {:.1}%)                 {}│{}",
            patina,
            reset,
            lf_stats.cache_stats.entry_count,
            lf_stats.cache_stats.hit_rate * 100.0,
            patina,
            reset
        );
        println!(
            "  {}│{}     Bandwidth Saved {:>8} bytes                              {}│{}",
            patina, reset, lf_stats.bandwidth_saved_bytes, patina, reset
        );
        println!(
            "  {}│{}     Status          {:>8}                                    {}│{}",
            patina, reset, lf_stats.offline_status, patina, reset
        );
        println!(
            "  {}│{}                                                                    {}│{}",
            patina, reset, patina, reset
        );

        // Concurrency governor stats
        let gov_stats = self.governor.stats();
        println!(
            "  {}│{}  {bold}{}⊘ CONCURRENCY{}{:<43}    {}│{}",
            patina, reset, sand, reset, "", patina, reset
        );
        println!(
            "  {}│{}     Streams         {:>8} / {:<8}                          {}│{}",
            patina, reset, gov_stats.streams_available, gov_stats.streams_max, patina, reset
        );
        println!(
            "  {}│{}     Tools           {:>8} / {:<8}                          {}│{}",
            patina, reset, gov_stats.tools_available, gov_stats.tools_max, patina, reset
        );
        println!(
            "  {}│{}     Global          {:>8} / {:<8}                          {}│{}",
            patina, reset, gov_stats.global_available, gov_stats.global_max, patina, reset
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
}
