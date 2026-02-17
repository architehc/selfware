//! Output Control Module
//!
//! Centralized output rendering based on CLI flags:
//! - `compact_mode`: Minimal output, no decorative chrome
//! - `verbose_mode`: Extra detail, show reasoning, debug info
//! - `show_tokens`: Display token usage after responses
//! - `show_mascot`: Display ASCII fox mascot during key moments

use colored::*;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

// Re-export mascot types for convenience
pub use crate::ui::mascot::{
    render_inline_mascot, render_mascot, render_mascot_with_message, MascotMood,
};

/// Global output mode flags (set once at startup)
static COMPACT_MODE: AtomicBool = AtomicBool::new(false);
static VERBOSE_MODE: AtomicBool = AtomicBool::new(false);
static SHOW_TOKENS: AtomicBool = AtomicBool::new(false);

/// Token counters for the session
static TOTAL_PROMPT_TOKENS: AtomicU64 = AtomicU64::new(0);
static TOTAL_COMPLETION_TOKENS: AtomicU64 = AtomicU64::new(0);

/// Initialize output modes from config
pub(crate) fn init(compact: bool, verbose: bool, show_tokens: bool) {
    COMPACT_MODE.store(compact, Ordering::SeqCst);
    VERBOSE_MODE.store(verbose, Ordering::SeqCst);
    SHOW_TOKENS.store(show_tokens, Ordering::SeqCst);
}

/// Check if compact mode is enabled
#[inline]
pub(crate) fn is_compact() -> bool {
    COMPACT_MODE.load(Ordering::SeqCst)
}

/// Check if verbose mode is enabled
#[inline]
pub(crate) fn is_verbose() -> bool {
    VERBOSE_MODE.load(Ordering::SeqCst)
}

/// Check if show_tokens is enabled
#[inline]
pub(crate) fn should_show_tokens() -> bool {
    SHOW_TOKENS.load(Ordering::SeqCst)
}

/// Record token usage
#[inline]
pub(crate) fn record_tokens(prompt: u64, completion: u64) {
    TOTAL_PROMPT_TOKENS.fetch_add(prompt, Ordering::SeqCst);
    TOTAL_COMPLETION_TOKENS.fetch_add(completion, Ordering::SeqCst);
}

/// Get total token usage
#[allow(dead_code)]
#[inline]
pub(crate) fn get_total_tokens() -> (u64, u64) {
    (
        TOTAL_PROMPT_TOKENS.load(Ordering::SeqCst),
        TOTAL_COMPLETION_TOKENS.load(Ordering::SeqCst),
    )
}

/// Reset token counters (for new sessions)
#[allow(dead_code)]
#[inline]
pub(crate) fn reset_tokens() {
    TOTAL_PROMPT_TOKENS.store(0, Ordering::SeqCst);
    TOTAL_COMPLETION_TOKENS.store(0, Ordering::SeqCst);
}

/// Print token usage summary
pub(crate) fn print_token_usage(prompt: u64, completion: u64) {
    if should_show_tokens() {
        let total = prompt + completion;
        if is_compact() {
            println!("{}", format!("[{} tokens]", total).dimmed());
        } else {
            println!(
                "{} {} prompt + {} completion = {} total",
                "📊 Tokens:".bright_blue(),
                prompt.to_string().cyan(),
                completion.to_string().cyan(),
                total.to_string().bright_cyan()
            );
        }
    }
}

/// Print session token summary (at end of session)
#[allow(dead_code)]
pub(crate) fn print_session_summary() {
    if should_show_tokens() {
        let (prompt, completion) = get_total_tokens();
        let total = prompt + completion;
        if total > 0 {
            println!();
            if is_compact() {
                println!("{}", format!("[Session: {} tokens]", total).dimmed());
            } else {
                println!(
                    "{} {} prompt + {} completion = {} total",
                    "📊 Session tokens:".bright_blue(),
                    prompt.to_string().cyan(),
                    completion.to_string().cyan(),
                    total.to_string().bright_cyan()
                );
            }
        }
    }
}

/// Print tool call announcement (verbose fallback)
pub(crate) fn tool_call(name: &str) {
    if !is_compact() {
        println!(
            "{} Calling tool: {}",
            "🔧".bright_blue(),
            name.bright_cyan()
        );
    }
}

/// Print tool success (verbose fallback)
pub(crate) fn tool_success(name: &str) {
    if !is_compact() {
        println!("{} Tool succeeded", "✓".bright_green());
    } else if is_verbose() {
        println!("{} {}", "✓".green(), name);
    }
}

/// Print tool failure (always shown, but format varies)
pub(crate) fn tool_failure(name: &str, error: &str) {
    if is_compact() {
        println!("{} {}: {}", "✗".red(), name, error);
    } else {
        println!("{} Tool failed: {}", "✗".bright_red(), error);
    }
}

// ============================================================================
// Semantic Tool Call Summaries
// ============================================================================

/// Extract a file path from tool arguments
fn extract_path(args: &serde_json::Value) -> Option<&str> {
    args.get("path")
        .or_else(|| args.get("file_path"))
        .or_else(|| args.get("file"))
        .and_then(|v| v.as_str())
}

/// Extract a command string from tool arguments
fn extract_command(args: &serde_json::Value) -> Option<&str> {
    args.get("command")
        .or_else(|| args.get("cmd"))
        .and_then(|v| v.as_str())
}

/// Extract a search pattern from tool arguments
fn extract_pattern(args: &serde_json::Value) -> Option<&str> {
    args.get("pattern")
        .or_else(|| args.get("query"))
        .or_else(|| args.get("search"))
        .and_then(|v| v.as_str())
}

/// Generate a one-line semantic summary for a tool call
pub(crate) fn semantic_summary(
    tool_name: &str,
    args: &serde_json::Value,
    result: Option<&str>,
    success: bool,
    duration_ms: u64,
) -> String {
    let path = extract_path(args).unwrap_or("?");
    let short_path = if path.len() > 50 {
        &path[path.len() - 50..]
    } else {
        path
    };

    match tool_name {
        "file_read" => {
            let lines = result
                .and_then(|r| {
                    // Try to count lines from the result
                    serde_json::from_str::<serde_json::Value>(r)
                        .ok()
                        .and_then(|v| {
                            v.get("content")
                                .and_then(|c| c.as_str().map(|s| s.lines().count()))
                        })
                })
                .unwrap_or(0);
            if lines > 0 {
                format!("Read {} ({} lines)", short_path, lines)
            } else {
                format!("Read {}", short_path)
            }
        }
        "file_write" | "file_create" => {
            let bytes = result
                .and_then(|r| {
                    serde_json::from_str::<serde_json::Value>(r)
                        .ok()
                        .and_then(|v| v.get("bytes_written").and_then(|b| b.as_u64()))
                })
                .unwrap_or(0);
            if bytes > 0 {
                format!("Wrote {} ({} bytes)", short_path, format_number(bytes))
            } else {
                format!("Wrote {}", short_path)
            }
        }
        "file_edit" => format!("Edited {}", short_path),
        "shell_exec" => {
            let cmd = extract_command(args).unwrap_or("?");
            let short_cmd = if cmd.len() > 40 { &cmd[..40] } else { cmd };
            let exit_code = result.and_then(|r| {
                serde_json::from_str::<serde_json::Value>(r)
                    .ok()
                    .and_then(|v| v.get("exit_code").and_then(|c| c.as_i64()))
            });
            match exit_code {
                Some(code) => format!("Ran: {} (exit {})", short_cmd, code),
                None => format!("Ran: {}", short_cmd),
            }
        }
        "cargo_test" => {
            if success {
                let passed = result
                    .and_then(|r| {
                        // Look for "X passed" in test output
                        r.find("passed").and_then(|idx| {
                            let before = &r[..idx];
                            before.rfind(char::is_whitespace).map(|i| &before[i + 1..])
                        })
                    })
                    .unwrap_or("all");
                format!("Tests: {} passed", passed)
            } else {
                "Tests: some failed".to_string()
            }
        }
        "cargo_check" => {
            if success {
                "Cargo check passed".to_string()
            } else {
                "Cargo check failed".to_string()
            }
        }
        "grep_search" | "ripgrep_search" => {
            let pattern = extract_pattern(args).unwrap_or("?");
            let short_pattern = if pattern.len() > 30 {
                &pattern[..30]
            } else {
                pattern
            };
            let matches = result
                .and_then(|r| {
                    serde_json::from_str::<serde_json::Value>(r)
                        .ok()
                        .and_then(|v| v.get("matches").and_then(|m| m.as_array().map(|a| a.len())))
                })
                .unwrap_or(0);
            if matches > 0 {
                format!("Searched '{}' ({} matches)", short_pattern, matches)
            } else {
                format!("Searched '{}'", short_pattern)
            }
        }
        "git_status" => {
            let changed = result
                .and_then(|r| {
                    serde_json::from_str::<serde_json::Value>(r)
                        .ok()
                        .and_then(|v| v.get("files").and_then(|f| f.as_array().map(|a| a.len())))
                })
                .unwrap_or(0);
            if changed > 0 {
                format!("Git status ({} changed)", changed)
            } else {
                "Git status (clean)".to_string()
            }
        }
        "git_diff" => {
            let lines = result
                .and_then(|r| {
                    serde_json::from_str::<serde_json::Value>(r)
                        .ok()
                        .and_then(|v| {
                            v.get("diff")
                                .and_then(|d| d.as_str().map(|s| s.lines().count()))
                        })
                })
                .unwrap_or(0);
            if lines > 0 {
                format!("Git diff ({} lines)", lines)
            } else {
                "Git diff".to_string()
            }
        }
        "directory_tree" => format!("Listed {}", short_path),
        "glob_find" => {
            let pattern = extract_pattern(args).unwrap_or("?");
            format!("Glob '{}'", pattern)
        }
        "git_log" => "Git log".to_string(),
        "git_commit" => "Git commit".to_string(),
        _ => format!("{} ({}ms)", tool_name, duration_ms),
    }
}

/// Format a number with comma separators
fn format_number(n: u64) -> String {
    if n >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{:.1}K", n as f64 / 1_000.0)
    } else {
        n.to_string()
    }
}

/// Print tool activity start line (shown while tool is running)
pub(crate) fn tool_activity_start(name: &str, args: &serde_json::Value) {
    if is_compact() {
        return;
    }
    if is_verbose() {
        // Verbose mode: use raw tool_call output
        tool_call(name);
        return;
    }
    // Normal mode: show semantic activity indicator
    let activity = match name {
        "file_read" => format!("Reading {}...", extract_path(args).unwrap_or("file")),
        "file_write" | "file_create" => {
            format!("Writing {}...", extract_path(args).unwrap_or("file"))
        }
        "file_edit" => format!("Editing {}...", extract_path(args).unwrap_or("file")),
        "shell_exec" => format!(
            "Running {}...",
            extract_command(args)
                .map(|c| if c.len() > 40 { &c[..40] } else { c })
                .unwrap_or("command")
        ),
        "cargo_test" => "Running tests...".to_string(),
        "cargo_check" => "Checking project...".to_string(),
        "grep_search" | "ripgrep_search" => {
            format!("Searching '{}'...", extract_pattern(args).unwrap_or("?"))
        }
        "git_status" => "Checking git status...".to_string(),
        "git_diff" => "Getting diff...".to_string(),
        "git_log" => "Reading git log...".to_string(),
        "git_commit" => "Committing...".to_string(),
        "directory_tree" => format!("Listing {}...", extract_path(args).unwrap_or(".")),
        "glob_find" => format!("Finding {}...", extract_pattern(args).unwrap_or("files")),
        _ => format!("{}...", name),
    };
    println!("  {}", activity.dimmed());
}

/// Print tool result summary (shown after tool completes)
pub(crate) fn tool_result_summary(summary: &str, success: bool) {
    if is_verbose() {
        // Verbose mode falls through to tool_success/tool_failure in caller
        return;
    }
    if is_compact() {
        if !success {
            println!("{} {}", "✗".red(), summary);
        }
        return;
    }
    // Normal mode: semantic one-liner
    if success {
        println!("  {} {}", "✓".bright_green(), summary);
    } else {
        println!("  {} {}", "✗".bright_red(), summary);
    }
}

/// Print safety check failure (always shown)
pub(crate) fn safety_blocked(message: &str) {
    println!("{} {}", "🚫".bright_red(), message);
}

/// Print thinking/reasoning output
pub(crate) fn thinking(text: &str, inline: bool) {
    // In compact mode, skip thinking entirely
    // In normal mode, show thinking
    // In verbose mode, show full thinking with emphasis
    if is_compact() {
        return;
    }

    if inline {
        if is_verbose() {
            print!("{}", text.bright_black());
        } else {
            print!("{}", text.dimmed());
        }
    } else if is_verbose() {
        println!(
            "{} {}",
            "💭 Thinking:".bright_magenta(),
            text.bright_black()
        );
    } else {
        println!("{} {}", "Thinking:".dimmed(), text.dimmed());
    }
}

/// Print thinking prefix (for streaming)
pub(crate) fn thinking_prefix() {
    if !is_compact() {
        print!("{} ", "Thinking:".dimmed());
    }
}

/// Print intent detection message
pub(crate) fn intent_without_action() {
    if !is_compact() {
        println!(
            "{}",
            "🔄 Model described intent but didn't act - prompting for action...".bright_yellow()
        );
    }
}

/// Print final answer
pub(crate) fn final_answer(content: &str) {
    if is_compact() {
        println!("{}", content);
    } else {
        println!("{} {}", "Final answer:".bright_green(), content);
    }
}

/// Print task completed message
pub(crate) fn task_completed() {
    if !is_compact() {
        println!("{}", "✅ Task completed successfully!".bright_green());
    }
}

/// Print task completed with mascot (verbose mode)
#[allow(dead_code)]
pub(crate) fn task_completed_with_mascot() {
    if is_verbose() {
        println!(
            "{}",
            render_mascot_with_message(MascotMood::Success, "Task completed successfully!")
        );
    } else if !is_compact() {
        println!("{}", "✅ Task completed successfully!".bright_green());
    }
}

/// Print task failed with mascot (verbose mode)
#[allow(dead_code)]
pub(crate) fn task_failed_with_mascot(reason: &str) {
    if is_verbose() {
        println!(
            "{}",
            render_mascot_with_message(MascotMood::Error, &format!("Task failed: {}", reason))
        );
    } else {
        println!("{} {}", "❌ Task failed:".bright_red(), reason);
    }
}

/// Print greeting mascot on startup (verbose mode only)
#[allow(dead_code)]
pub(crate) fn greeting_mascot() {
    if is_verbose() {
        println!("{}", render_mascot(MascotMood::Greeting));
    }
}

/// Print thinking mascot during LLM calls (verbose mode only)
#[allow(dead_code)]
pub(crate) fn thinking_mascot() {
    if is_verbose() {
        println!("{}", render_inline_mascot(MascotMood::Thinking));
    }
}

/// Print working mascot during tool execution (verbose mode only)
#[allow(dead_code)]
pub(crate) fn working_mascot() {
    if is_verbose() {
        print!("{} ", render_inline_mascot(MascotMood::Working));
    }
}

/// Print verification report
pub(crate) fn verification_report(report: &str, passed: bool) {
    if is_verbose() {
        // Full report in verbose mode
        println!("{}", report);
    } else if !is_compact() {
        // Summary in normal mode
        if passed {
            println!("{}", "✓ Verification passed".bright_green());
        } else {
            // Always show failures
            println!("{}", report);
        }
    } else {
        // Compact: only show failures
        if !passed {
            println!("{}", report);
        }
    }
}

/// Print debug output (only in verbose mode or with SELFWARE_DEBUG)
pub(crate) fn debug_output(label: &str, content: &str) {
    if is_verbose() || std::env::var("SELFWARE_DEBUG").is_ok() {
        println!("{}", format!("=== DEBUG: {} ===", label).bright_magenta());
        println!("{}", content);
        println!("{}", "=== END DEBUG ===".bright_magenta());
    }
}

/// Print confirmation prompt preview
#[allow(dead_code)]
pub(crate) fn confirmation_preview(tool_name: &str, args: &str) {
    println!(
        "{} Tool: {} Args: {}",
        "⚠️".bright_yellow(),
        tool_name.bright_cyan(),
        args.bright_white()
    );
}

// ============================================================================
// Multi-Phase Progress Display
// ============================================================================

use std::time::{Duration, Instant};

/// Phase status for progress tracking
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PhaseStatus {
    Pending,
    Active,
    Completed,
    Failed,
}

/// A phase in the multi-step progress
#[derive(Debug, Clone)]
pub struct ProgressPhase {
    pub name: String,
    pub status: PhaseStatus,
    pub progress: f64,
}

/// Multi-step progress tracker with ETA
pub struct TaskProgress {
    phases: Vec<ProgressPhase>,
    current_phase: usize,
    start_time: Instant,
}

impl TaskProgress {
    /// Create a new task progress tracker with given phase names
    pub(crate) fn new(phase_names: &[&str]) -> Self {
        Self {
            phases: phase_names
                .iter()
                .map(|name| ProgressPhase {
                    name: name.to_string(),
                    status: PhaseStatus::Pending,
                    progress: 0.0,
                })
                .collect(),
            current_phase: 0,
            start_time: Instant::now(),
        }
    }

    /// Start the current phase
    pub(crate) fn start_phase(&mut self) {
        if self.current_phase < self.phases.len() {
            self.phases[self.current_phase].status = PhaseStatus::Active;
            self.print_progress();
        }
    }

    /// Update progress of current phase (0.0 to 1.0)
    pub(crate) fn update_progress(&mut self, progress: f64) {
        if self.current_phase < self.phases.len() {
            self.phases[self.current_phase].progress = progress.clamp(0.0, 1.0);
            // Only print in verbose mode for incremental updates
            if is_verbose() {
                self.print_progress();
            }
        }
    }

    /// Complete current phase and move to next
    pub(crate) fn complete_phase(&mut self) {
        if self.current_phase < self.phases.len() {
            self.phases[self.current_phase].status = PhaseStatus::Completed;
            self.phases[self.current_phase].progress = 1.0;
            self.current_phase += 1;
            if self.current_phase < self.phases.len() {
                self.phases[self.current_phase].status = PhaseStatus::Active;
            }
            self.print_progress();
        }
    }

    /// Mark current phase as failed
    pub(crate) fn fail_phase(&mut self) {
        if self.current_phase < self.phases.len() {
            self.phases[self.current_phase].status = PhaseStatus::Failed;
            self.print_progress();
        }
    }

    /// Add a new phase dynamically
    #[allow(dead_code)]
    pub(crate) fn add_phase(&mut self, name: &str) {
        self.phases.push(ProgressPhase {
            name: name.to_string(),
            status: PhaseStatus::Pending,
            progress: 0.0,
        });
    }

    /// Get overall progress (0.0 to 1.0)
    pub(crate) fn overall_progress(&self) -> f64 {
        if self.phases.is_empty() {
            return 0.0;
        }
        let completed: f64 = self
            .phases
            .iter()
            .map(|p| match p.status {
                PhaseStatus::Completed => 1.0,
                PhaseStatus::Active => p.progress,
                _ => 0.0,
            })
            .sum();
        completed / self.phases.len() as f64
    }

    /// Estimate remaining time based on elapsed time and progress
    pub(crate) fn estimated_remaining(&self) -> Option<Duration> {
        let progress = self.overall_progress();
        if progress > 0.05 {
            let elapsed = self.start_time.elapsed();
            let estimated_total = elapsed.as_secs_f64() / progress;
            let remaining = estimated_total - elapsed.as_secs_f64();
            if remaining > 0.0 {
                return Some(Duration::from_secs_f64(remaining));
            }
        }
        None
    }

    /// Format ETA as human-readable string
    fn format_eta(&self) -> Option<String> {
        self.estimated_remaining().map(|remaining| {
            let secs = remaining.as_secs();
            if secs >= 60 {
                format!("~{}m {}s", secs / 60, secs % 60)
            } else {
                format!("~{}s", secs)
            }
        })
    }

    /// Print current progress state
    pub(crate) fn print_progress(&self) {
        if is_compact() {
            // Compact: single line with overall progress
            let progress = self.overall_progress();
            let pct = if progress.is_finite() {
                (progress.clamp(0.0, 1.0) * 100.0).round() as u32
            } else {
                0
            };
            let current_name = self
                .phases
                .get(self.current_phase)
                .map(|p| p.name.as_str())
                .unwrap_or("Done");
            if let Some(eta) = self.format_eta() {
                println!("[{}% {} ETA:{}]", pct, current_name, eta);
            } else {
                println!("[{}% {}]", pct, current_name);
            }
        } else if is_verbose() {
            // Verbose: full multi-line progress with all phases
            println!();
            for (i, phase) in self.phases.iter().enumerate() {
                let (icon, name_color) = match phase.status {
                    PhaseStatus::Pending => ("○".dimmed(), phase.name.dimmed()),
                    PhaseStatus::Active => ("●".bright_cyan(), phase.name.bright_white()),
                    PhaseStatus::Completed => ("✓".bright_green(), phase.name.green()),
                    PhaseStatus::Failed => ("✗".bright_red(), phase.name.red()),
                };

                let progress_str = if phase.status == PhaseStatus::Active && phase.progress > 0.0 {
                    format!(" [{:.0}%]", phase.progress * 100.0)
                        .cyan()
                        .to_string()
                } else {
                    String::new()
                };

                println!(
                    "  {} {}/{} {}{}",
                    icon,
                    (i + 1).to_string().dimmed(),
                    self.phases.len().to_string().dimmed(),
                    name_color,
                    progress_str
                );
            }

            // Show ETA
            if let Some(eta) = self.format_eta() {
                println!("  {} {}", "ETA:".dimmed(), eta.bright_cyan());
            }
            println!();
        } else {
            // Normal: show current phase with progress bar
            if let Some(phase) = self.phases.get(self.current_phase) {
                let pct = {
                    let p = self.overall_progress();
                    if !p.is_finite() {
                        0
                    } else {
                        (p.clamp(0.0, 1.0) * 100.0).round() as u32
                    }
                };
                let bar_width = 20;
                let filled = (pct as usize * bar_width) / 100;
                let empty = bar_width - filled;
                let bar = format!(
                    "{}{}",
                    "█".repeat(filled).bright_cyan(),
                    "░".repeat(empty).dimmed()
                );

                let eta_str = self
                    .format_eta()
                    .map(|e| format!(" ETA: {}", e.cyan()))
                    .unwrap_or_default();

                println!(
                    "{} [{}/{}] {} [{}] {}%{}",
                    "📊".bright_blue(),
                    (self.current_phase + 1).to_string().bright_white(),
                    self.phases.len().to_string().dimmed(),
                    phase.name.bright_white(),
                    bar,
                    pct.to_string().bright_cyan(),
                    eta_str
                );
            }
        }
    }
}

/// Print step announcement (used by agent)
pub(crate) fn step_start(step: usize, name: &str) {
    if is_compact() {
        print!("[Step {}] ", step);
    } else {
        println!(
            "{} {}...",
            format!("📝 Step {}", step).bright_blue(),
            name.bright_white()
        );
    }
}

/// Print phase transition
pub(crate) fn phase_transition(from: &str, to: &str) {
    if is_verbose() {
        println!(
            "{} {} → {}",
            "🔄".bright_yellow(),
            from.dimmed(),
            to.bright_white()
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    // Mutex to serialize tests that access global token state
    static TOKEN_TEST_MUTEX: Mutex<()> = Mutex::new(());

    #[test]
    fn test_init_and_check_modes() {
        init(true, false, true);
        assert!(is_compact());
        assert!(!is_verbose());
        assert!(should_show_tokens());

        init(false, true, false);
        assert!(!is_compact());
        assert!(is_verbose());
        assert!(!should_show_tokens());
    }

    #[test]
    fn test_token_tracking() {
        let _lock = TOKEN_TEST_MUTEX.lock().unwrap_or_else(|e| e.into_inner());
        reset_tokens();
        record_tokens(100, 50);
        record_tokens(200, 100);

        let (prompt, completion) = get_total_tokens();
        assert_eq!(prompt, 300);
        assert_eq!(completion, 150);
    }

    #[test]
    fn test_reset_tokens() {
        let _lock = TOKEN_TEST_MUTEX.lock().unwrap_or_else(|e| e.into_inner());
        reset_tokens();
        record_tokens(100, 50);
        reset_tokens();

        let (prompt, completion) = get_total_tokens();
        assert_eq!(prompt, 0);
        assert_eq!(completion, 0);
    }

    #[test]
    fn test_task_progress_creation() {
        let progress = TaskProgress::new(&["Planning", "Executing", "Verifying"]);
        assert_eq!(progress.phases.len(), 3);
        assert_eq!(progress.overall_progress(), 0.0);
    }

    #[test]
    fn test_task_progress_phases() {
        let mut progress = TaskProgress::new(&["Phase 1", "Phase 2"]);

        // Start first phase
        progress.start_phase();
        assert_eq!(progress.phases[0].status, PhaseStatus::Active);

        // Complete first phase
        progress.complete_phase();
        assert_eq!(progress.phases[0].status, PhaseStatus::Completed);
        assert_eq!(progress.phases[1].status, PhaseStatus::Active);

        // Check overall progress (50% = 1 out of 2 phases)
        assert!((progress.overall_progress() - 0.5).abs() < 0.01);

        // Complete second phase
        progress.complete_phase();
        assert_eq!(progress.overall_progress(), 1.0);
    }

    #[test]
    fn test_task_progress_update() {
        let mut progress = TaskProgress::new(&["Build"]);
        progress.start_phase();

        progress.update_progress(0.5);
        assert!((progress.phases[0].progress - 0.5).abs() < 0.01);

        // Clamp values
        progress.update_progress(1.5);
        assert!((progress.phases[0].progress - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_task_progress_failure() {
        let mut progress = TaskProgress::new(&["Test"]);
        progress.start_phase();
        progress.fail_phase();
        assert_eq!(progress.phases[0].status, PhaseStatus::Failed);
    }

    #[test]
    fn test_semantic_summary_file_read() {
        let args = serde_json::json!({"path": "src/main.rs"});
        let summary = semantic_summary("file_read", &args, None, true, 50);
        assert!(summary.contains("Read"));
        assert!(summary.contains("src/main.rs"));
    }

    #[test]
    fn test_semantic_summary_file_write() {
        let args = serde_json::json!({"path": "src/lib.rs"});
        let summary = semantic_summary("file_write", &args, None, true, 50);
        assert!(summary.contains("Wrote"));
        assert!(summary.contains("src/lib.rs"));
    }

    #[test]
    fn test_semantic_summary_file_edit() {
        let args = serde_json::json!({"path": "src/main.rs"});
        let summary = semantic_summary("file_edit", &args, None, true, 50);
        assert!(summary.contains("Edited"));
        assert!(summary.contains("src/main.rs"));
    }

    #[test]
    fn test_semantic_summary_shell_exec() {
        let args = serde_json::json!({"command": "cargo build"});
        let summary = semantic_summary("shell_exec", &args, None, true, 100);
        assert!(summary.contains("Ran"));
        assert!(summary.contains("cargo build"));
    }

    #[test]
    fn test_semantic_summary_cargo_check() {
        let args = serde_json::json!({});
        let summary = semantic_summary("cargo_check", &args, None, true, 200);
        assert_eq!(summary, "Cargo check passed");
    }

    #[test]
    fn test_semantic_summary_grep_search() {
        let args = serde_json::json!({"pattern": "TODO"});
        let summary = semantic_summary("grep_search", &args, None, true, 30);
        assert!(summary.contains("Searched"));
        assert!(summary.contains("TODO"));
    }

    #[test]
    fn test_semantic_summary_git_status() {
        let args = serde_json::json!({});
        let summary = semantic_summary("git_status", &args, None, true, 20);
        assert!(summary.contains("Git status"));
    }

    #[test]
    fn test_semantic_summary_unknown_tool() {
        let args = serde_json::json!({});
        let summary = semantic_summary("unknown_tool", &args, None, true, 150);
        assert!(summary.contains("unknown_tool"));
        assert!(summary.contains("150ms"));
    }

    #[test]
    fn test_format_number() {
        assert_eq!(format_number(500), "500");
        assert_eq!(format_number(1500), "1.5K");
        assert_eq!(format_number(1_500_000), "1.5M");
    }

    #[test]
    fn test_extract_path() {
        let args = serde_json::json!({"path": "src/main.rs"});
        assert_eq!(extract_path(&args), Some("src/main.rs"));

        let args2 = serde_json::json!({"file_path": "lib.rs"});
        assert_eq!(extract_path(&args2), Some("lib.rs"));

        let empty = serde_json::json!({});
        assert_eq!(extract_path(&empty), None);
    }

    #[test]
    fn test_extract_command() {
        let args = serde_json::json!({"command": "cargo test"});
        assert_eq!(extract_command(&args), Some("cargo test"));

        let empty = serde_json::json!({});
        assert_eq!(extract_command(&empty), None);
    }

    #[test]
    fn test_extract_pattern() {
        let args = serde_json::json!({"pattern": "TODO"});
        assert_eq!(extract_pattern(&args), Some("TODO"));

        let args2 = serde_json::json!({"query": "search term"});
        assert_eq!(extract_pattern(&args2), Some("search term"));
    }

    #[test]
    fn test_task_progress_add_phase() {
        let mut progress = TaskProgress::new(&["Phase 1"]);
        assert_eq!(progress.phases.len(), 1);

        progress.add_phase("Phase 2");
        assert_eq!(progress.phases.len(), 2);
        assert_eq!(progress.phases[1].name, "Phase 2");
        assert_eq!(progress.phases[1].status, PhaseStatus::Pending);
    }
}
