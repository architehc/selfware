//! Output Control Module
//!
//! Centralized output rendering based on CLI flags:
//! - `compact_mode`: Minimal output, no decorative chrome
//! - `verbose_mode`: Extra detail, show reasoning, debug info
//! - `show_tokens`: Display token usage after responses

use colored::*;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

/// Global output mode flags (set once at startup)
static COMPACT_MODE: AtomicBool = AtomicBool::new(false);
static VERBOSE_MODE: AtomicBool = AtomicBool::new(false);
static SHOW_TOKENS: AtomicBool = AtomicBool::new(false);

/// Token counters for the session
static TOTAL_PROMPT_TOKENS: AtomicU64 = AtomicU64::new(0);
static TOTAL_COMPLETION_TOKENS: AtomicU64 = AtomicU64::new(0);

/// Initialize output modes from config
pub fn init(compact: bool, verbose: bool, show_tokens: bool) {
    COMPACT_MODE.store(compact, Ordering::SeqCst);
    VERBOSE_MODE.store(verbose, Ordering::SeqCst);
    SHOW_TOKENS.store(show_tokens, Ordering::SeqCst);
}

/// Check if compact mode is enabled
pub fn is_compact() -> bool {
    COMPACT_MODE.load(Ordering::SeqCst)
}

/// Check if verbose mode is enabled
pub fn is_verbose() -> bool {
    VERBOSE_MODE.load(Ordering::SeqCst)
}

/// Check if show_tokens is enabled
pub fn should_show_tokens() -> bool {
    SHOW_TOKENS.load(Ordering::SeqCst)
}

/// Record token usage
pub fn record_tokens(prompt: u64, completion: u64) {
    TOTAL_PROMPT_TOKENS.fetch_add(prompt, Ordering::SeqCst);
    TOTAL_COMPLETION_TOKENS.fetch_add(completion, Ordering::SeqCst);
}

/// Get total token usage
pub fn get_total_tokens() -> (u64, u64) {
    (
        TOTAL_PROMPT_TOKENS.load(Ordering::SeqCst),
        TOTAL_COMPLETION_TOKENS.load(Ordering::SeqCst),
    )
}

/// Reset token counters (for new sessions)
pub fn reset_tokens() {
    TOTAL_PROMPT_TOKENS.store(0, Ordering::SeqCst);
    TOTAL_COMPLETION_TOKENS.store(0, Ordering::SeqCst);
}

/// Print token usage summary
pub fn print_token_usage(prompt: u64, completion: u64) {
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
pub fn print_session_summary() {
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
pub fn tool_call(name: &str) {
    if !is_compact() {
        println!(
            "{} Calling tool: {}",
            "🔧".bright_blue(),
            name.bright_cyan()
        );
    }
}

/// Print tool success
pub fn tool_success(name: &str) {
    if !is_compact() {
        println!("{} Tool succeeded", "✓".bright_green());
    } else if is_verbose() {
        println!("{} {}", "✓".green(), name);
    }
}

/// Print tool failure (always shown, but format varies)
pub fn tool_failure(name: &str, error: &str) {
    if is_compact() {
        println!("{} {}: {}", "✗".red(), name, error);
    } else {
        println!("{} Tool failed: {}", "✗".bright_red(), error);
    }
}

/// Print safety check failure (always shown)
pub fn safety_blocked(message: &str) {
    println!("{} {}", "🚫".bright_red(), message);
}

/// Print thinking/reasoning output
pub fn thinking(text: &str, inline: bool) {
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
    } else {
        if is_verbose() {
            println!("{} {}", "💭 Thinking:".bright_magenta(), text.bright_black());
        } else {
            println!("{} {}", "Thinking:".dimmed(), text.dimmed());
        }
    }
}

/// Print thinking prefix (for streaming)
pub fn thinking_prefix() {
    if !is_compact() {
        print!("{} ", "Thinking:".dimmed());
    }
}

/// Print intent detection message
pub fn intent_without_action() {
    if !is_compact() {
        println!(
            "{}",
            "🔄 Model described intent but didn't act - prompting for action..."
                .bright_yellow()
        );
    }
}

/// Print final answer
pub fn final_answer(content: &str) {
    if is_compact() {
        println!("{}", content);
    } else {
        println!("{} {}", "Final answer:".bright_green(), content);
    }
}

/// Print task completed message
pub fn task_completed() {
    if !is_compact() {
        println!("{}", "✅ Task completed successfully!".bright_green());
    }
}

/// Print verification report
pub fn verification_report(report: &str, passed: bool) {
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
pub fn debug_output(label: &str, content: &str) {
    if is_verbose() || std::env::var("SELFWARE_DEBUG").is_ok() {
        println!("{}", format!("=== DEBUG: {} ===", label).bright_magenta());
        println!("{}", content);
        println!("{}", "=== END DEBUG ===".bright_magenta());
    }
}

/// Print confirmation prompt preview
pub fn confirmation_preview(tool_name: &str, args: &str) {
    println!(
        "{} Tool: {} Args: {}",
        "⚠️".bright_yellow(),
        tool_name.bright_cyan(),
        args.bright_white()
    );
}

#[cfg(test)]
mod tests {
    use super::*;

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
        reset_tokens();
        record_tokens(100, 50);
        record_tokens(200, 100);

        let (prompt, completion) = get_total_tokens();
        assert_eq!(prompt, 300);
        assert_eq!(completion, 150);
    }

    #[test]
    fn test_reset_tokens() {
        record_tokens(100, 50);
        reset_tokens();

        let (prompt, completion) = get_total_tokens();
        assert_eq!(prompt, 0);
        assert_eq!(completion, 0);
    }
}
