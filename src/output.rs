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
pub(crate) fn is_compact() -> bool {
    COMPACT_MODE.load(Ordering::SeqCst)
}

/// Check if verbose mode is enabled
pub(crate) fn is_verbose() -> bool {
    VERBOSE_MODE.load(Ordering::SeqCst)
}

/// Check if show_tokens is enabled
pub(crate) fn should_show_tokens() -> bool {
    SHOW_TOKENS.load(Ordering::SeqCst)
}

/// Record token usage
pub(crate) fn record_tokens(prompt: u64, completion: u64) {
    TOTAL_PROMPT_TOKENS.fetch_add(prompt, Ordering::SeqCst);
    TOTAL_COMPLETION_TOKENS.fetch_add(completion, Ordering::SeqCst);
}

/// Get total token usage
pub(crate) fn get_total_tokens() -> (u64, u64) {
    (
        TOTAL_PROMPT_TOKENS.load(Ordering::SeqCst),
        TOTAL_COMPLETION_TOKENS.load(Ordering::SeqCst),
    )
}

/// Reset token counters (for new sessions)
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

/// Print tool call announcement
pub(crate) fn tool_call(name: &str) {
    if !is_compact() {
        println!(
            "{} Calling tool: {}",
            "🔧".bright_blue(),
            name.bright_cyan()
        );
    }
}

/// Print tool success
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
pub(crate) fn greeting_mascot() {
    if is_verbose() {
        println!("{}", render_mascot(MascotMood::Greeting));
    }
}

/// Print thinking mascot during LLM calls (verbose mode only)
pub(crate) fn thinking_mascot() {
    if is_verbose() {
        println!("{}", render_inline_mascot(MascotMood::Thinking));
    }
}

/// Print working mascot during tool execution (verbose mode only)
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
    fn test_task_progress_add_phase() {
        let mut progress = TaskProgress::new(&["Phase 1"]);
        assert_eq!(progress.phases.len(), 1);

        progress.add_phase("Phase 2");
        assert_eq!(progress.phases.len(), 2);
        assert_eq!(progress.phases[1].name, "Phase 2");
        assert_eq!(progress.phases[1].status, PhaseStatus::Pending);
    }
}
