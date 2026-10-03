use colored::*;

use super::*;

/// Compact token count: `999`, `21.7k`, `164k`.
pub(crate) fn format_tokens_k(tokens: usize) -> String {
    if tokens < 1_000 {
        tokens.to_string()
    } else if tokens < 100_000 {
        format!("{:.1}k", tokens as f64 / 1000.0)
    } else {
        format!("{:.0}k", tokens as f64 / 1000.0)
    }
}

/// Share of the model's context window in use, 0–100 (0 when the window is
/// unknown).
pub(crate) fn context_pct(used: usize, window: usize) -> f64 {
    if window == 0 {
        0.0
    } else {
        (used as f64 / window as f64 * 100.0).min(100.0)
    }
}

/// The one context-usage wording shared by the status bar, `/ctx`, `/stats`
/// and the startup line: usage against the MODEL context window, plus the
/// separate compaction threshold (the history budget that triggers
/// compression). The status bar used the window (13.3%) while `/stats` used
/// the compaction budget (20.4%) for the same 21742 tokens — two
/// percentages, neither labelled (0.9.1 field test).
///
/// `21.7k of 164k context (13%) · compaction at 106k`
pub(crate) fn context_usage_label(used: usize, window: usize, compaction_at: usize) -> String {
    let mut label = if window == 0 {
        format!("{} context (window unknown)", format_tokens_k(used))
    } else {
        let pct = context_pct(used, window);
        let pct = if used > 0 && pct < 1.0 {
            "<1%".to_string()
        } else {
            format!("{pct:.0}%")
        };
        format!(
            "{} of {} context ({pct})",
            format_tokens_k(used),
            format_tokens_k(window)
        )
    };
    if compaction_at > 0 && (window == 0 || compaction_at < window) {
        label.push_str(&format!(
            " · compaction at {}",
            format_tokens_k(compaction_at)
        ));
    }
    label
}

/// Shorten `s` to at most `max_chars` characters, marking the cut with `…`
/// instead of chopping mid-name silently (`[qwen38-flash-ne]`).
pub(crate) fn truncate_with_ellipsis(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        return s.to_string();
    }
    if max_chars == 0 {
        return String::new();
    }
    let mut out: String = s.chars().take(max_chars - 1).collect();
    out.push('…');
    out
}

/// Layout of the pre-prompt status line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct StatusBarLayout {
    /// Draw the 10-cell usage bar (dropped first when space is short).
    pub show_bar: bool,
    /// Model name as displayed (ellipsized only when the line cannot fit).
    pub model: String,
    /// Spaces between the left hint and the right-hand status.
    pub padding: usize,
}

/// Width of the usage bar plus its trailing space.
const STATUS_BAR_CELLS: usize = 11;
/// Shortest model name worth showing before it stops identifying anything.
const MIN_MODEL_CHARS: usize = 12;

/// Fit `left` + (bar) + `middle` + ` [model]` into `term_width` columns:
/// drop the bar first, then ellipsize the model, never cutting mid-name
/// without a marker. `middle` is the context label plus any cost.
pub(crate) fn layout_status_bar(
    left: &str,
    middle: &str,
    model: &str,
    term_width: usize,
) -> StatusBarLayout {
    use crate::ui::components::visible_width;
    // Leading space before `left`, two before the right side.
    let fixed = 1 + visible_width(left) + 2 + visible_width(middle);
    let model_cols = |m: &str| 3 + visible_width(m); // " [" + m + "]"
    let fits = |bar: bool, m: &str| {
        fixed + if bar { STATUS_BAR_CELLS } else { 0 } + model_cols(m) <= term_width
    };
    let (show_bar, model) = if fits(true, model) {
        (true, model.to_string())
    } else if fits(false, model) {
        (false, model.to_string())
    } else {
        let room = term_width.saturating_sub(fixed + 3);
        (
            false,
            truncate_with_ellipsis(model, room.max(MIN_MODEL_CHARS)),
        )
    };
    let used = fixed + if show_bar { STATUS_BAR_CELLS } else { 0 } + model_cols(&model);
    StatusBarLayout {
        show_bar,
        model,
        padding: term_width.saturating_sub(used).max(1),
    }
}

impl Agent {
    /// `context_usage_label` for the current conversation.
    pub(crate) fn context_usage_text(&self) -> String {
        context_usage_label(
            self.total_tokens_used(),
            self.memory.context_window(),
            self.compaction_threshold(),
        )
    }

    /// The history size (estimated tokens) at which compaction starts: the
    /// compressor's own threshold, `max_context_tokens × context_content_ratio`.
    /// The ONE number every "compaction at" display and the `/compact`
    /// target read — the status bar used to show `max_context_tokens` (the
    /// hard trim budget), which on a 1M window read "compaction at 796k"
    /// while compaction actually started at 597k.
    pub fn compaction_threshold(&self) -> usize {
        self.compressor.compression_threshold()
    }

    /// Print the status line before the prompt:
    ///
    /// ` [normal] ? for shortcuts      ██░░░░░░░░ 21.7k of 164k context (13%) · compaction at 106k [model]`
    ///
    /// A dollar amount appears only when the provider reported a cost (the
    /// same session fold as `/cost`); it used to come from a hard-coded
    /// price table and showed `$0.07` for an endpoint that bills nothing.
    pub(super) fn print_status_bar(&self) {
        use colored::*;

        let pct = self.context_usage_pct();

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

        let cost = self.session_usage().status_bar_cost();
        let label = self.context_usage_text();
        let middle = match &cost {
            Some(c) => format!("{label} {c}"),
            None => label.clone(),
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
        let layout = layout_status_bar(&left, &middle, &self.config.model, term_width);

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

        let bar_part = if layout.show_bar {
            format!("{} ", colored_bar)
        } else {
            String::new()
        };
        let cost_part = cost.map(|c| format!(" {}", c.dimmed())).unwrap_or_default();

        println!(
            " {} {}{}{}  {}{}{} [{}]",
            mode_colored,
            "? for shortcuts".dimmed(),
            trust_colored,
            " ".repeat(layout.padding),
            bar_part,
            label,
            cost_part,
            layout.model.dimmed(),
        );
    }

    /// Show compact startup context line (Claude Code style)
    pub(super) fn show_startup_context(&self) {
        let tool_count = self.tools.list().len();
        let cwd = crate::tools::workspace_root::current_path()
            .display()
            .to_string();
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

        let short_model = truncate_with_ellipsis(&self.config.model, 32);

        println!(
            "  {} {}  {} {}  {} {}  {} {}",
            "Model:".dimmed(),
            short_model.bright_cyan(),
            "Context:".dimmed(),
            self.context_usage_text(),
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
        let used_pct = context_pct(tokens, window);
        let compaction_at = self.compaction_threshold();
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
        // The compaction threshold is a separate, smaller budget than the
        // model window; /stats used to divide by it without saying so.
        println!(
            "  {}│{}     {}⇲{}  {}compaction{}    {}{:>10}{} tokens (history budget)      {}│{}",
            patina,
            reset,
            coral,
            reset,
            worn,
            reset,
            patina_light,
            compaction_at,
            reset,
            patina,
            reset
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
                    .find(|message| {
                        super::context_files::is_context_file_message(
                            message,
                            std::path::Path::new(path_str),
                        )
                    })
                    .map(|m| crate::token_count::estimate_tokens_with_overhead(m.content.text(), 4))
                    .unwrap_or(0);
                total_file_tokens += file_tokens;
                let is_stale = self.file_tracker.is_stale(path_str);
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
        // Same measurement and denominators as the status bar and /ctx.
        let context_label = self.context_usage_text();
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

        // Rows are framed by `frame_box`, which pads each row by its display
        // width — the hand-padded box broke whenever a value or glyph
        // changed width (UX field test, 0.9.0).
        let tc_stats = self.cache_manager.tool_cache.stats().await;
        let lf_stats = self.cache_manager.local_first.stats();
        let gov_stats = self.governor.stats();
        let mode_str = match self.execution_mode() {
            crate::config::ExecutionMode::Normal => "NORMAL - Confirm all tools",
            crate::config::ExecutionMode::AutoEdit => "AUTO-EDIT - Auto-approve file ops",
            crate::config::ExecutionMode::Yolo => "YOLO - Execute without confirmation",
            crate::config::ExecutionMode::Daemon => "DAEMON - Permanent auto-execute",
        };
        let heading = |colour: &str, label: &str| format!("{bold}{colour}{label}{reset}");
        let rows = vec![
            String::new(),
            heading(rust, "◈ CONTEXT"),
            format!("    Context         {context_label}"),
            format!(
                "    Messages        {:>8}  (user: {}, assistant: {})",
                messages, user_msgs, assistant_msgs
            ),
            format!("    Tool Calls      {:>8}", tool_calls),
            String::new(),
            heading(sand, "⊡ MEMORY"),
            format!("    Entries         {:>8}", self.memory.len()),
            format!(
                "    Files Loaded    {:>8}",
                self.file_tracker.context_files.len()
            ),
            format!("    Session         {:>8}", session_indicator),
            String::new(),
            heading(sand, "◇ TOOL CACHE"),
            format!(
                "    Entries         {:>8} / {:<8}",
                tc_stats.entries, tc_stats.max_entries
            ),
            format!("    TTL             {:>8}s", tc_stats.default_ttl_secs),
            String::new(),
            heading(sand, "◆ LOCAL-FIRST"),
            format!(
                "    Cache Entries   {:>8}  (hit rate: {:.1}%)",
                lf_stats.cache_stats.entry_count,
                lf_stats.cache_stats.hit_rate * 100.0
            ),
            format!(
                "    Bandwidth Saved {:>8} bytes",
                lf_stats.bandwidth_saved_bytes
            ),
            format!("    Status          {:>8}", lf_stats.offline_status),
            String::new(),
            heading(sand, "⊘ CONCURRENCY"),
            format!(
                "    Streams         {:>8} / {:<8}",
                gov_stats.streams_available, gov_stats.streams_max
            ),
            format!(
                "    Tools           {:>8} / {:<8}",
                gov_stats.tools_available, gov_stats.tools_max
            ),
            format!(
                "    Global          {:>8} / {:<8}",
                gov_stats.global_available, gov_stats.global_max
            ),
            String::new(),
            heading(worn, "≋ MODE"),
            format!("    {}", mode_str),
            String::new(),
        ];
        let title = format!("{rust}SESSION STATS{patina}");
        println!();
        for line in crate::ui::components::frame_box(&title, &rows, 68, patina, reset) {
            println!("  {line}");
        }
        println!();
    }
}

#[cfg(test)]
#[path = "../../tests/unit/agent/context_display/context_display_test.rs"]
mod tests;
