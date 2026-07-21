//! Output Control Module
//!
//! Centralized output rendering based on CLI flags:
//! - `compact_mode`: Minimal output, no decorative chrome
//! - `verbose_mode`: Extra detail, show reasoning, debug info
//! - `show_tokens`: Display token usage after responses
//! - `show_mascot`: Display ASCII fox mascot during key moments

use colored::*;
use std::io::{self, Write};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;

/// Global output lock to prevent interleaving from concurrent tasks.
/// All output functions should acquire this lock before printing.
pub static OUTPUT_LOCK: Mutex<()> = Mutex::new(());

/// Global output mode flags (set once at startup)
static COMPACT_MODE: AtomicBool = AtomicBool::new(false);
static VERBOSE_MODE: AtomicBool = AtomicBool::new(false);
static SHOW_TOKENS: AtomicBool = AtomicBool::new(false);

/// When true, all print functions become no-ops — the TUI owns rendering.
static TUI_ACTIVE: AtomicBool = AtomicBool::new(false);

/// When true, the user requested quiet mode and non-essential output is suppressed.
static QUIET_MODE: AtomicBool = AtomicBool::new(false);

/// When true, output is machine-readable JSON — suppress all human-oriented
/// stdout (banners, "Final answer:", ANSI escapes) so only valid JSON goes out.
static JSON_MODE: AtomicBool = AtomicBool::new(false);

/// When true, assistant responses are streamed live to stdout token-by-token,
/// so `final_answer` must not re-print the content (it would duplicate it).
static STREAMING_MODE: AtomicBool = AtomicBool::new(false);

/// When true, stdout is not a terminal (piped/captured) — strip ANSI and emoji.
static PLAIN_MODE: AtomicBool = AtomicBool::new(false);

/// Returns true when stdout is not a terminal (piped/captured/redirected).
fn stdout_is_tty() -> bool {
    // `IsTerminal` is stable since Rust 1.70.
    use std::io::IsTerminal;
    io::stdout().is_terminal()
}

/// Returns true when output should be plain (no ANSI colors, no emoji).
pub(crate) fn is_plain_mode() -> bool {
    PLAIN_MODE.load(Ordering::Relaxed)
}

/// Set the TUI active flag. Call with `true` when the TUI launches.
pub fn set_tui_active(active: bool) {
    TUI_ACTIVE.store(active, Ordering::SeqCst);
}

/// Returns true when the TUI is rendering — print functions should be suppressed.
pub fn is_tui_active() -> bool {
    TUI_ACTIVE.load(Ordering::Relaxed)
}

/// Set the global quiet flag. Call once from the CLI after parsing arguments.
pub(crate) fn set_quiet(quiet: bool) {
    QUIET_MODE.store(quiet, Ordering::SeqCst);
}

/// Set the JSON output mode flag.  When true, all human-oriented print
/// functions become no-ops so that only valid JSON is emitted on stdout.
pub(crate) fn set_json_mode(json: bool) {
    JSON_MODE.store(json, Ordering::SeqCst);
}

pub(crate) fn set_streaming_mode(streaming: bool) {
    STREAMING_MODE.store(streaming, Ordering::SeqCst);
}

/// True when responses stream live to stdout (so `final_answer` skips re-print).
pub(crate) fn is_streaming_mode() -> bool {
    STREAMING_MODE.load(Ordering::Relaxed)
}

/// Returns true when JSON output mode is enabled (json or stream-json).
pub(crate) fn is_json_mode() -> bool {
    JSON_MODE.load(Ordering::Relaxed)
}

/// Returns true when quiet mode is enabled.
pub(crate) fn is_quiet() -> bool {
    QUIET_MODE.load(Ordering::Relaxed)
}

/// True when any output-suppressing mode is active.
pub(crate) fn should_suppress_output() -> bool {
    is_quiet() || is_tui_active() || is_json_mode()
}

/// Token counters for the session
static TOTAL_PROMPT_TOKENS: AtomicU64 = AtomicU64::new(0);
static TOTAL_COMPLETION_TOKENS: AtomicU64 = AtomicU64::new(0);

const SHELL_SUMMARY_PREVIEW_CHARS: usize = 120;

/// Initialize output modes from config
pub(crate) fn init(compact: bool, verbose: bool, show_tokens: bool) {
    COMPACT_MODE.store(compact, Ordering::SeqCst);
    VERBOSE_MODE.store(verbose, Ordering::SeqCst);
    SHOW_TOKENS.store(show_tokens, Ordering::SeqCst);

    // Detect whether stdout is a real terminal. When it is not (output is
    // piped, captured, or redirected), disable ANSI colour codes globally so
    // that downstream parsers don't see escape sequences, and enable plain
    // mode to strip emoji glyphs from messages.
    if !stdout_is_tty() {
        PLAIN_MODE.store(true, Ordering::SeqCst);
        colored::control::set_override(false);
    }
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
    if is_quiet() || is_json_mode() {
        return;
    }
    if should_show_tokens() {
        let _lock = OUTPUT_LOCK.lock().unwrap_or_else(|e| e.into_inner());
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
    let short_path = if path.chars().count() > 50 {
        let skip = path.chars().count() - 50;
        &path[path.char_indices().nth(skip).map(|(i, _)| i).unwrap_or(0)..]
    } else {
        path
    };

    // Helper closure to parse result JSON
    let result_json = |r: Option<&str>| -> Option<serde_json::Value> {
        r.and_then(|s| serde_json::from_str(s).ok())
    };

    match tool_name {
        // === File operations ===
        "file_read" => {
            let lines = result_json(result)
                .and_then(|v| {
                    v.get("content")
                        .and_then(|c| c.as_str().map(|s| s.lines().count()))
                })
                .unwrap_or(0);
            if lines > 0 {
                format!("Read {} ({} lines)", short_path, lines)
            } else {
                format!("Read {}", short_path)
            }
        }
        "file_write" | "file_create" => {
            let bytes = result_json(result)
                .and_then(|v| v.get("bytes_written").and_then(|b| b.as_u64()))
                .unwrap_or(0);
            if bytes > 0 {
                format!("Wrote {} ({} bytes)", short_path, format_number(bytes))
            } else {
                format!("Wrote {}", short_path)
            }
        }
        "file_edit" => format!("Edited {}", short_path),
        "file_delete" => format!("Deleted {}", short_path),

        // === Shell ===
        "shell_exec" => {
            let cmd = extract_command(args).unwrap_or("?");
            let short_cmd = if cmd.chars().count() > SHELL_SUMMARY_PREVIEW_CHARS {
                &cmd[..cmd
                    .char_indices()
                    .nth(SHELL_SUMMARY_PREVIEW_CHARS)
                    .map(|(i, _)| i)
                    .unwrap_or(cmd.len())]
            } else {
                cmd
            };
            let exit_code =
                result_json(result).and_then(|v| v.get("exit_code").and_then(|c| c.as_i64()));
            match exit_code {
                Some(code) => format!("Ran: {} (exit {})", short_cmd, code),
                None => format!("Ran: {}", short_cmd),
            }
        }

        // === Cargo / Build ===
        "cargo_test" => {
            if success {
                let passed = result
                    .and_then(|r| {
                        r.find("passed").and_then(|idx| {
                            let before = r[..idx].trim_end();
                            before.rsplit_once(char::is_whitespace).map(|(_, n)| n)
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
        "cargo_clippy" => {
            if success {
                "Clippy: clean".to_string()
            } else {
                "Clippy: warnings".to_string()
            }
        }
        "cargo_fmt" => {
            if success {
                "Formatted code".to_string()
            } else {
                "Format check failed".to_string()
            }
        }

        // === Search ===
        "grep_search" | "ripgrep_search" => {
            let pattern = extract_pattern(args).unwrap_or("?");
            let short_pattern = if pattern.chars().count() > 30 {
                pattern.chars().take(30).collect::<String>()
            } else {
                pattern.to_string()
            };
            let matches = result_json(result)
                .and_then(|v| v.get("matches").and_then(|m| m.as_array().map(|a| a.len())))
                .unwrap_or(0);
            if matches > 0 {
                format!("Searched '{}' ({} matches)", short_pattern, matches)
            } else {
                format!("Searched '{}'", short_pattern)
            }
        }
        "symbol_search" => {
            let pattern = extract_pattern(args).unwrap_or("?");
            format!("Symbol search '{}'", pattern)
        }
        "glob_find" => {
            let pattern = extract_pattern(args).unwrap_or("?");
            format!("Glob '{}'", pattern)
        }

        // === Git ===
        "git_status" => {
            let rj = result_json(result);
            let staged = rj
                .as_ref()
                .and_then(|v| v.get("staged").and_then(|a| a.as_array().map(|a| a.len())))
                .unwrap_or(0);
            let unstaged = rj
                .as_ref()
                .and_then(|v| {
                    v.get("unstaged")
                        .and_then(|a| a.as_array().map(|a| a.len()))
                })
                .unwrap_or(0);
            let untracked = rj
                .as_ref()
                .and_then(|v| {
                    v.get("untracked")
                        .and_then(|a| a.as_array().map(|a| a.len()))
                })
                .unwrap_or(0);
            let total = staged + unstaged + untracked;
            if total > 0 {
                format!("Git status ({} changed)", total)
            } else {
                "Git status (clean)".to_string()
            }
        }
        "git_diff" => {
            let lines = result_json(result)
                .and_then(|v| {
                    v.get("diff")
                        .and_then(|d| d.as_str().map(|s| s.lines().count()))
                })
                .unwrap_or(0);
            if lines > 0 {
                format!("Git diff ({} lines)", lines)
            } else {
                "Git diff".to_string()
            }
        }
        "git_log" => "Git log".to_string(),
        "git_commit" => "Git commit".to_string(),
        "git_checkpoint" => {
            let msg = args
                .get("message")
                .and_then(|v| v.as_str())
                .unwrap_or("checkpoint");
            format!("Checkpoint: {}", msg)
        }
        "git_push" => {
            let remote = args
                .get("remote")
                .and_then(|v| v.as_str())
                .unwrap_or("origin");
            let branch = result_json(result)
                .and_then(|v| {
                    v.get("branch")
                        .and_then(|b| b.as_str().map(|s| s.to_string()))
                })
                .unwrap_or_default();
            if branch.is_empty() {
                format!("Pushed to {}", remote)
            } else {
                format!("Pushed {} to {}", branch, remote)
            }
        }

        // === Directory ===
        "directory_tree" => format!("Listed {}", short_path),

        // === HTTP ===
        "http_request" => {
            let method = args.get("method").and_then(|v| v.as_str()).unwrap_or("GET");
            let url = args.get("url").and_then(|v| v.as_str()).unwrap_or("?");
            let short_url = if url.chars().count() > 40 {
                url.chars().take(40).collect::<String>()
            } else {
                url.to_string()
            };
            format!("HTTP {} {}", method, short_url)
        }

        // === Process management ===
        "process_start" => {
            let cmd = extract_command(args).unwrap_or("process");
            format!("Started {}", cmd)
        }
        "process_stop" => {
            let id = args
                .get("id")
                .or_else(|| args.get("name"))
                .and_then(|v| v.as_str())
                .unwrap_or("process");
            format!("Stopped {}", id)
        }
        "process_list" => "Process list".to_string(),
        "process_logs" => {
            let id = args
                .get("id")
                .or_else(|| args.get("name"))
                .and_then(|v| v.as_str())
                .unwrap_or("process");
            format!("Logs for {}", id)
        }
        "process_restart" => {
            let id = args
                .get("id")
                .or_else(|| args.get("name"))
                .and_then(|v| v.as_str())
                .unwrap_or("process");
            format!("Restarted {}", id)
        }
        "port_check" => {
            let port = args.get("port").and_then(|v| v.as_u64()).unwrap_or(0);
            format!("Port {} check", port)
        }

        // === Container operations ===
        "container_run" => {
            let image = args
                .get("image")
                .and_then(|v| v.as_str())
                .unwrap_or("container");
            format!("Container run {}", image)
        }
        "container_stop" => {
            let id = args
                .get("container")
                .or_else(|| args.get("id"))
                .and_then(|v| v.as_str())
                .unwrap_or("container");
            format!("Container stop {}", id)
        }
        "container_list" => "Container list".to_string(),
        "container_logs" => {
            let id = args
                .get("container")
                .or_else(|| args.get("id"))
                .and_then(|v| v.as_str())
                .unwrap_or("container");
            format!("Container logs {}", id)
        }
        "container_exec" => {
            let id = args
                .get("container")
                .or_else(|| args.get("id"))
                .and_then(|v| v.as_str())
                .unwrap_or("container");
            format!("Container exec {}", id)
        }
        "container_build" => {
            let tag = args.get("tag").and_then(|v| v.as_str()).unwrap_or("image");
            format!("Container build {}", tag)
        }
        "container_images" => "Container images".to_string(),
        "container_pull" => {
            let image = args
                .get("image")
                .and_then(|v| v.as_str())
                .unwrap_or("image");
            format!("Container pull {}", image)
        }
        "container_remove" => {
            let id = args
                .get("container")
                .or_else(|| args.get("id"))
                .and_then(|v| v.as_str())
                .unwrap_or("container");
            format!("Container remove {}", id)
        }
        "compose_up" => "Compose up".to_string(),
        "compose_down" => "Compose down".to_string(),

        // === Package managers ===
        "npm_install" => {
            let pkg = args.get("package").and_then(|v| v.as_str());
            match pkg {
                Some(p) => format!("npm install {}", p),
                None => "npm install".to_string(),
            }
        }
        "npm_run" => {
            let script = args
                .get("script")
                .and_then(|v| v.as_str())
                .unwrap_or("script");
            format!("npm run {}", script)
        }
        "npm_scripts" => "npm scripts".to_string(),
        "pip_install" => {
            let pkg = args.get("package").and_then(|v| v.as_str());
            match pkg {
                Some(p) => format!("pip install {}", p),
                None => "pip install".to_string(),
            }
        }
        "pip_list" => "pip list".to_string(),
        "pip_freeze" => "pip freeze".to_string(),
        "yarn_install" => {
            let pkg = args.get("package").and_then(|v| v.as_str());
            match pkg {
                Some(p) => format!("yarn add {}", p),
                None => "yarn install".to_string(),
            }
        }

        // === Browser automation ===
        "browser_fetch" => {
            let url = args.get("url").and_then(|v| v.as_str()).unwrap_or("?");
            let short_url = if url.chars().count() > 40 {
                url.chars().take(40).collect::<String>()
            } else {
                url.to_string()
            };
            format!("Fetch {}", short_url)
        }
        "browser_screenshot" => {
            let url = args.get("url").and_then(|v| v.as_str()).unwrap_or("page");
            let short_url = if url.chars().count() > 40 {
                url.chars().take(40).collect::<String>()
            } else {
                url.to_string()
            };
            format!("Screenshot {}", short_url)
        }
        "browser_pdf" => {
            let url = args.get("url").and_then(|v| v.as_str()).unwrap_or("page");
            let short_url = if url.chars().count() > 40 {
                url.chars().take(40).collect::<String>()
            } else {
                url.to_string()
            };
            format!("PDF {}", short_url)
        }
        "browser_eval" => "Browser eval".to_string(),
        "browser_links" => {
            let url = args.get("url").and_then(|v| v.as_str()).unwrap_or("page");
            let short_url = if url.chars().count() > 40 {
                url.chars().take(40).collect::<String>()
            } else {
                url.to_string()
            };
            format!("Links from {}", short_url)
        }

        // === Knowledge graph ===
        "knowledge_add" => {
            let entity = args
                .get("name")
                .or_else(|| args.get("entity"))
                .and_then(|v| v.as_str())
                .unwrap_or("entity");
            format!("Knowledge add '{}'", entity)
        }
        "knowledge_relate" => "Knowledge relate".to_string(),
        "knowledge_query" => {
            let query = extract_pattern(args).unwrap_or("?");
            format!("Knowledge query '{}'", query)
        }
        "knowledge_stats" => "Knowledge stats".to_string(),
        "knowledge_clear" => "Knowledge cleared".to_string(),
        "knowledge_remove" => {
            let entity = args
                .get("name")
                .or_else(|| args.get("entity"))
                .and_then(|v| v.as_str())
                .unwrap_or("entity");
            format!("Knowledge remove '{}'", entity)
        }
        "knowledge_export" => "Knowledge export".to_string(),

        // === Fallback ===
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

/// Generate the activity message for a running tool
pub(crate) fn tool_activity_message(name: &str, args: &serde_json::Value) -> String {
    match name {
        "file_read" => format!("Reading {}...", extract_path(args).unwrap_or("file")),
        "file_write" | "file_create" => {
            format!("Writing {}...", extract_path(args).unwrap_or("file"))
        }
        "file_edit" => format!("Editing {}...", extract_path(args).unwrap_or("file")),
        "file_delete" => format!("Deleting {}...", extract_path(args).unwrap_or("file")),
        "shell_exec" => format!(
            "Running {}...",
            extract_command(args)
                .map(|c| {
                    if c.chars().count() > SHELL_SUMMARY_PREVIEW_CHARS {
                        c.chars()
                            .take(SHELL_SUMMARY_PREVIEW_CHARS)
                            .collect::<String>()
                    } else {
                        c.to_string()
                    }
                })
                .unwrap_or_else(|| "command".to_string())
        ),
        "cargo_test" => "Running tests...".to_string(),
        "cargo_check" => "Checking project...".to_string(),
        "cargo_clippy" => "Running clippy...".to_string(),
        "cargo_fmt" => "Formatting code...".to_string(),
        "grep_search" | "ripgrep_search" => {
            format!("Searching '{}'...", extract_pattern(args).unwrap_or("?"))
        }
        "symbol_search" => format!(
            "Searching symbols '{}'...",
            extract_pattern(args).unwrap_or("?")
        ),
        "git_status" => "Checking git status...".to_string(),
        "git_diff" => "Getting diff...".to_string(),
        "git_log" => "Reading git log...".to_string(),
        "git_commit" => "Committing...".to_string(),
        "git_push" => "Pushing...".to_string(),
        "git_checkpoint" => "Creating checkpoint...".to_string(),
        "directory_tree" => format!("Listing {}...", extract_path(args).unwrap_or(".")),
        "glob_find" => format!("Finding {}...", extract_pattern(args).unwrap_or("files")),
        "http_request" => "Making HTTP request...".to_string(),
        "process_start" => "Starting process...".to_string(),
        "process_stop" => "Stopping process...".to_string(),
        "process_list" => "Listing processes...".to_string(),
        "process_logs" => "Fetching logs...".to_string(),
        "process_restart" => "Restarting process...".to_string(),
        "container_run" | "container_build" => "Running container...".to_string(),
        "container_stop" | "container_remove" => "Stopping container...".to_string(),
        "npm_install" | "pip_install" | "yarn_install" => "Installing packages...".to_string(),
        "npm_run" => "Running script...".to_string(),
        "browser_fetch" => "Fetching page...".to_string(),
        "browser_screenshot" => "Taking screenshot...".to_string(),
        "knowledge_add" | "knowledge_relate" => "Updating knowledge...".to_string(),
        "knowledge_query" => "Querying knowledge...".to_string(),
        _ => format!("{}...", name),
    }
}

/// Print safety check failure
pub(crate) fn safety_blocked(message: &str) {
    // Raw stdout corrupts ratatui's alternate-screen frame. The safety result
    // is already delivered through the agent event/message path in TUI mode.
    if should_suppress_output() {
        return;
    }
    let _lock = OUTPUT_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    println!("{} {}", "🚫".bright_red(), message);
}

/// Print thinking/reasoning output
pub(crate) fn thinking(text: &str, inline: bool) {
    // should_suppress_output() covers quiet + JSON + TUI. The TUI case is
    // critical: a raw stdout print here corrupts the rendered frame (reasoning
    // text bleeds across pane borders). is_compact() is additionally suppressed.
    if is_compact() || should_suppress_output() {
        return;
    }

    let _lock = OUTPUT_LOCK.lock().unwrap_or_else(|e| e.into_inner());

    // Replace \n with \r\n so newlines in thinking text reset to column 0
    let safe = text.replace('\n', "\r\n");
    if is_plain_mode() {
        // Plain mode: no ANSI escape codes
        if inline {
            print!("{}", safe);
        } else {
            print!("Thinking: {}\r\n", safe);
        }
        io::stdout().flush().ok();
    } else if inline {
        if is_verbose() {
            print!("{}", safe.bright_black());
        } else {
            print!("{}", safe.dimmed());
        }
        io::stdout().flush().ok();
    } else if is_verbose() {
        print!(
            "\r\x1b[2K{} {}\r\n",
            "💭 Thinking:".bright_magenta(),
            safe.bright_black()
        );
        io::stdout().flush().ok();
    } else {
        print!("\r\x1b[2K{} {}\r\n", "Thinking:".dimmed(), safe.dimmed());
        io::stdout().flush().ok();
    }
}

/// Print thinking prefix (for streaming)
pub(crate) fn thinking_prefix() {
    if should_suppress_output() {
        return;
    }
    if !is_compact() {
        let _lock = OUTPUT_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        if is_plain_mode() {
            print!("Thinking: ");
        } else {
            print!("\r\x1b[2K{} ", "Thinking:".dimmed());
        }
        io::stdout().flush().ok();
    }
}

/// Print intent detection message
#[cfg(test)]
pub(crate) fn intent_without_action() {
    if should_suppress_output() {
        return;
    }
    if !is_compact() {
        let _lock = OUTPUT_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        print!(
            "\r\x1b[2K{}\n",
            "🔄 Model described intent but didn't act - prompting for action...".bright_yellow()
        );
        io::stdout().flush().ok();
    }
}

/// Show detailed no-action recovery info (verbose mode or when content is short).
pub(crate) fn intent_without_action_detail(
    model_said: &str,
    correction: &str,
    attempt: usize,
    total: usize,
) {
    if is_tui_active() || is_compact() || is_quiet() || is_json_mode() {
        return;
    }
    if !is_verbose() {
        return;
    }
    let _lock = OUTPUT_LOCK.lock().unwrap_or_else(|e| e.into_inner());

    // Show what the model said (truncated)
    let preview: String = model_said.chars().take(120).collect();
    let truncated = if model_said.len() > 120 { "…" } else { "" };
    print!(
        "\r\x1b[2K  {} {}{}\n",
        "Model:".dimmed(),
        preview.dimmed(),
        truncated.dimmed()
    );

    // Show what correction is being sent
    let corr_preview: String = correction
        .lines()
        .next()
        .unwrap_or("")
        .chars()
        .take(100)
        .collect();
    print!(
        "\r\x1b[2K  {} {} ({}/{})\n",
        "Action:".bright_yellow(),
        corr_preview,
        attempt,
        total
    );
    io::stdout().flush().ok();
}

/// Show what the smart fallback decided to do.
pub(crate) fn smart_fallback_action(tool_name: &str, tool_args: &str) {
    if is_tui_active() || is_compact() || is_quiet() || is_json_mode() {
        return;
    }
    let _lock = OUTPUT_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let args_preview: String = tool_args.chars().take(80).collect();
    if is_plain_mode() {
        // Plain rendering: no ANSI, no emoji glyph
        print!("\r  Auto: {} {}\n", tool_name, args_preview);
    } else {
        print!(
            "\r\x1b[2K  {} {} {}\n",
            "↪ Auto:".bright_cyan(),
            tool_name.bright_cyan(),
            args_preview.dimmed()
        );
    }
    io::stdout().flush().ok();
}

/// Print final answer
pub(crate) fn final_answer(content: &str) {
    if should_suppress_output() {
        return;
    }
    // In streaming mode the assistant response was already displayed live,
    // token-by-token, on stdout; re-printing it here duplicates the final
    // answer on screen. (JSON/quiet/TUI are already handled above.)
    if is_streaming_mode() {
        return;
    }
    let _lock = OUTPUT_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    if is_plain_mode() || is_json_mode() {
        // In plain/JSON mode, emit only the content with no prefix or
        // decoration so machine-readable output is not corrupted.
        println!("{}", content);
    } else if is_compact() {
        print!("\r\x1b[2K{}\n", content);
    } else {
        print!("\r\x1b[2K{} {}\n", "Final answer:".bright_green(), content);
    }
    io::stdout().flush().ok();
}

/// Display a color-coded diff for file edits/writes.
/// Shows deleted lines in red and added lines in green.
pub(crate) fn display_file_diff(path: &str, old_content: &str, new_content: &str) {
    if is_tui_active() || is_compact() || is_quiet() || is_json_mode() {
        return;
    }
    let _lock = OUTPUT_LOCK.lock().unwrap_or_else(|e| e.into_inner());

    let old_lines: Vec<&str> = old_content.lines().collect();
    let new_lines: Vec<&str> = new_content.lines().collect();

    // Simple line-by-line diff — find changed regions
    let mut diff_lines: Vec<String> = Vec::new();
    let max_diff_lines = 30; // Limit display to avoid flooding

    // Use a basic LCS-style diff
    let mut i = 0;
    let mut j = 0;
    while i < old_lines.len() || j < new_lines.len() {
        if diff_lines.len() >= max_diff_lines {
            diff_lines.push(format!("  {} more changes...", "…".dimmed()));
            break;
        }

        if i < old_lines.len() && j < new_lines.len() && old_lines[i] == new_lines[j] {
            // Lines match — skip (don't show unchanged lines unless near a change)
            i += 1;
            j += 1;
            continue;
        }

        // Find the next sync point
        let mut found_sync = false;
        for lookahead in 1..10 {
            if j + lookahead < new_lines.len()
                && i < old_lines.len()
                && old_lines[i] == new_lines[j + lookahead]
            {
                // Added lines
                for line in new_lines.iter().skip(j).take(lookahead) {
                    diff_lines.push(format!("  {} {}", "+".bright_green(), line.bright_green()));
                }
                j += lookahead;
                found_sync = true;
                break;
            }
            if i + lookahead < old_lines.len()
                && j < new_lines.len()
                && old_lines[i + lookahead] == new_lines[j]
            {
                // Deleted lines
                for line in old_lines.iter().skip(i).take(lookahead) {
                    diff_lines.push(format!("  {} {}", "-".bright_red(), line.bright_red()));
                }
                i += lookahead;
                found_sync = true;
                break;
            }
        }

        if !found_sync {
            // Changed line (delete old + add new)
            if i < old_lines.len() {
                diff_lines.push(format!(
                    "  {} {}",
                    "-".bright_red(),
                    old_lines[i].bright_red()
                ));
                i += 1;
            }
            if j < new_lines.len() {
                diff_lines.push(format!(
                    "  {} {}",
                    "+".bright_green(),
                    new_lines[j].bright_green()
                ));
                j += 1;
            }
        }
    }

    if !diff_lines.is_empty() {
        if is_plain_mode() {
            println!("\r  --- {}", path);
            for line in &diff_lines {
                println!("\r{}", line);
            }
            println!("\r  ---");
        } else {
            println!("\r\x1b[2K  {} {}", "┌─".dimmed(), path.dimmed());
            for line in &diff_lines {
                println!("\r\x1b[2K{}", line);
            }
            println!("\r\x1b[2K  {}", "└─".dimmed());
        }
        io::stdout().flush().ok();
    }
}

/// Print task completed message.
///
/// Replaced by the structured failure-mode classifier in
/// `src/agent/failure_mode.rs::cli_banner` for the main agent path.
/// Kept here as `#[allow(dead_code)]` because integration tests and
/// future entry points may still want a generic completion banner.
#[allow(dead_code)]
pub(crate) fn task_completed() {
    if is_quiet() || is_compact() || is_json_mode() {
        return;
    }
    let _lock = OUTPUT_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    if is_plain_mode() {
        println!("Task completed successfully!");
    } else {
        println!("{}", "✅ Task completed successfully!".bright_green());
    }
}

/// Print verification report
pub(crate) fn verification_report(report: &str, passed: bool) {
    if should_suppress_output() {
        return;
    }
    let _lock = OUTPUT_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    // Replace \n with \r\n for proper column reset
    let safe = report.replace('\n', "\r\n");
    if is_verbose() {
        // Full report in verbose mode
        print!("{}\r\n", safe);
        io::stdout().flush().ok();
    } else if !is_compact() {
        // Summary in normal mode
        if passed {
            if is_plain_mode() {
                print!("\rVerification passed\r\n");
            } else {
                print!("\r\x1b[2K{}\r\n", "✓ Verification passed".bright_green());
            }
            io::stdout().flush().ok();
        } else {
            // Always show failures
            print!("{}\r\n", safe);
            io::stdout().flush().ok();
        }
    } else {
        // Compact: only show failures
        if !passed {
            print!("{}\r\n", safe);
            io::stdout().flush().ok();
        }
    }
}

/// Print debug output for per-turn LLM responses.
///
/// Gated on the unified [`crate::config::DebugConfig`] `turns` channel so the
/// `--debug=turns` CLI flag (and the legacy `SELFWARE_DEBUG` /
/// `SELFWARE_DEBUG_TURNS` env vars) actually disable this output when not set.
/// Verbose mode (`-v`) is preserved as a friendly opt-in for interactive use.
pub(crate) fn debug_output(debug: &crate::config::DebugConfig, label: &str, content: &str) {
    if is_quiet() {
        return;
    }
    if is_verbose() || debug.should_log_turns() {
        let _lock = OUTPUT_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        println!("{}", format!("=== DEBUG: {} ===", label).bright_magenta());
        println!("{}", content);
        println!("{}", "=== END DEBUG ===".bright_magenta());
    }
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
        // Suppress in quiet, TUI, and JSON modes — a progress bar on stdout
        // pollutes JSON output and corrupts a TUI frame.
        if should_suppress_output() {
            return;
        }
        let _lock = OUTPUT_LOCK.lock().unwrap_or_else(|e| e.into_inner());
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
                let (icon, name_color): (String, String) = if is_plain_mode() {
                    match phase.status {
                        PhaseStatus::Pending => ("o".to_string(), phase.name.clone()),
                        PhaseStatus::Active => ("*".to_string(), phase.name.clone()),
                        PhaseStatus::Completed => ("+".to_string(), phase.name.clone()),
                        PhaseStatus::Failed => ("x".to_string(), phase.name.clone()),
                    }
                } else {
                    match phase.status {
                        PhaseStatus::Pending => {
                            ("○".dimmed().to_string(), phase.name.dimmed().to_string())
                        }
                        PhaseStatus::Active => (
                            "●".bright_cyan().to_string(),
                            phase.name.bright_white().to_string(),
                        ),
                        PhaseStatus::Completed => (
                            "✓".bright_green().to_string(),
                            phase.name.green().to_string(),
                        ),
                        PhaseStatus::Failed => {
                            ("✗".bright_red().to_string(), phase.name.red().to_string())
                        }
                    }
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

                if is_plain_mode() {
                    let eta_str = self
                        .format_eta()
                        .map(|e| format!(" ETA: {}", e))
                        .unwrap_or_default();
                    print!(
                        "\r[{}/{}] {} {}%{}\n",
                        self.current_phase + 1,
                        self.phases.len(),
                        phase.name,
                        pct,
                        eta_str
                    );
                } else {
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

                    print!(
                        "\r\x1b[2K{} [{}/{}] {} [{}] {}%{}\n",
                        "📊".bright_blue(),
                        (self.current_phase + 1).to_string().bright_white(),
                        self.phases.len().to_string().dimmed(),
                        phase.name.bright_white(),
                        bar,
                        pct.to_string().bright_cyan(),
                        eta_str
                    );
                }
                io::stdout().flush().ok();
            }
        }
    }
}

/// Print step announcement (used by agent)
pub(crate) fn step_start(step: usize, name: &str) {
    if should_suppress_output() {
        return;
    }
    let _lock = OUTPUT_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    if is_plain_mode() {
        println!("[Step {}] {}...", step, name);
    } else if is_compact() {
        print!("\r\x1b[2K[Step {}] ", step);
    } else {
        print!(
            "\r\x1b[2K{} {}...\n",
            format!("📝 Step {}", step).bright_blue(),
            name.bright_white()
        );
    }
    io::stdout().flush().ok();
}

/// Print phase transition
pub(crate) fn phase_transition(from: &str, to: &str) {
    if !is_quiet() && is_verbose() {
        let _lock = OUTPUT_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        println!(
            "{} {} → {}",
            "🔄".bright_yellow(),
            from.dimmed(),
            to.bright_white()
        );
    }
}

#[cfg(test)]
#[path = "../../tests/unit/output/mod_test.rs"]
mod tests;
