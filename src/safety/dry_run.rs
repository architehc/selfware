//! Dry-run mode for previewing operations without executing them

// Feature-gated module

use crate::tool_parser::ParsedToolCall;
use colored::*;
use serde_json::Value;

/// Dry-run configuration
#[derive(Debug, Clone)]
pub struct DryRunConfig {
    /// Whether dry-run mode is enabled
    pub enabled: bool,
    /// Whether to show detailed tool arguments
    pub show_arguments: bool,
    /// Whether to show what would be modified
    pub show_diff_preview: bool,
    /// Maximum argument length to display
    pub max_arg_display_len: usize,
}

impl Default for DryRunConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            show_arguments: true,
            show_diff_preview: true,
            max_arg_display_len: 200,
        }
    }
}

/// Result of a dry-run preview
#[derive(Debug, Clone)]
pub struct DryRunPreview {
    pub tool_name: String,
    pub description: String,
    pub arguments: Value,
    pub would_modify: Vec<String>,
    pub risk_assessment: String,
}

/// Preview what a tool call would do without executing it
pub fn preview_tool_call(
    tool_name: &str,
    arguments: &Value,
    config: &DryRunConfig,
) -> DryRunPreview {
    let (description, would_modify, risk) = match tool_name {
        "file_read" => {
            let path = arguments
                .get("path")
                .and_then(|v| v.as_str())
                .unwrap_or("?");
            (
                format!("Read file: {}", path),
                vec![],
                "Safe - read-only operation".to_string(),
            )
        }
        "file_write" => {
            let path = arguments
                .get("path")
                .and_then(|v| v.as_str())
                .unwrap_or("?");
            let content_len = arguments
                .get("content")
                .and_then(|v| v.as_str())
                .map(|s| s.len())
                .unwrap_or(0);
            (
                format!("Write {} bytes to: {}", content_len, path),
                vec![path.to_string()],
                "Modifies filesystem".to_string(),
            )
        }
        "file_edit" => {
            let path = arguments
                .get("path")
                .and_then(|v| v.as_str())
                .unwrap_or("?");
            let old_str = arguments
                .get("old_str")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let new_str = arguments
                .get("new_str")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            (
                format!(
                    "Edit file: {} (replace {} chars with {} chars)",
                    path,
                    old_str.len(),
                    new_str.len()
                ),
                vec![path.to_string()],
                "Modifies existing file".to_string(),
            )
        }
        "directory_tree" => {
            let path = arguments
                .get("path")
                .and_then(|v| v.as_str())
                .unwrap_or(".");
            (
                format!("List directory: {}", path),
                vec![],
                "Safe - read-only operation".to_string(),
            )
        }
        "shell_exec" => {
            let cmd = arguments
                .get("command")
                .and_then(|v| v.as_str())
                .unwrap_or("?");
            let risk = if cmd.contains("rm") || cmd.contains("delete") {
                "HIGH - potentially destructive command"
            } else if cmd.contains(">") || cmd.contains("mv") || cmd.contains("cp") {
                "MEDIUM - modifies filesystem"
            } else {
                "Variable - depends on command"
            };
            (
                format!("Execute: {}", truncate_str(cmd, 60)),
                vec!["(depends on command)".to_string()],
                risk.to_string(),
            )
        }
        "git_commit" => {
            let msg = arguments
                .get("message")
                .and_then(|v| v.as_str())
                .unwrap_or("?");
            (
                format!("Git commit: {}", truncate_str(msg, 50)),
                vec![".git/".to_string()],
                "Safe - creates new commit".to_string(),
            )
        }
        "git_push" => {
            let force = arguments
                .get("force")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let risk = if force {
                "HIGH - force push can overwrite history"
            } else {
                "MEDIUM - pushes to remote"
            };
            (
                format!("Git push{}", if force { " --force" } else { "" }),
                vec!["remote repository".to_string()],
                risk.to_string(),
            )
        }
        "cargo_test" => (
            "Run cargo test".to_string(),
            vec![],
            "Safe - runs tests".to_string(),
        ),
        "cargo_check" => (
            "Run cargo check".to_string(),
            vec![],
            "Safe - checks compilation".to_string(),
        ),
        "cargo_clippy" => (
            "Run cargo clippy".to_string(),
            vec![],
            "Safe - runs linter".to_string(),
        ),
        "http_request" => {
            let url = arguments.get("url").and_then(|v| v.as_str()).unwrap_or("?");
            let method = arguments
                .get("method")
                .and_then(|v| v.as_str())
                .unwrap_or("GET");
            (
                format!("{} {}", method, truncate_str(url, 50)),
                vec![],
                "Network request - external communication".to_string(),
            )
        }
        "grep_search" | "glob_find" | "symbol_search" => {
            let pattern = arguments
                .get("pattern")
                .and_then(|v| v.as_str())
                .unwrap_or("?");
            (
                format!("Search: {}", pattern),
                vec![],
                "Safe - read-only search".to_string(),
            )
        }
        _ => (
            format!("Call tool: {}", tool_name),
            vec![],
            "Unknown - review arguments".to_string(),
        ),
    };

    DryRunPreview {
        tool_name: tool_name.to_string(),
        description,
        arguments: if config.show_arguments {
            arguments.clone()
        } else {
            Value::Null
        },
        would_modify,
        risk_assessment: risk,
    }
}

/// Display a dry-run preview to the user
pub fn display_preview(preview: &DryRunPreview, config: &DryRunConfig) {
    println!();
    println!("{}", "═══ DRY RUN PREVIEW ═══".yellow().bold());
    println!("{}: {}", "Tool".cyan(), preview.tool_name);
    println!("{}: {}", "Action".cyan(), preview.description);

    if !preview.would_modify.is_empty() {
        println!(
            "{}: {}",
            "Would modify".cyan(),
            preview.would_modify.join(", ")
        );
    }

    println!(
        "{}: {}",
        "Risk".cyan(),
        colorize_risk(&preview.risk_assessment)
    );

    if config.show_arguments && preview.arguments != Value::Null {
        let args_str = serde_json::to_string_pretty(&preview.arguments).unwrap_or_default();
        let args_display = truncate_str(&args_str, config.max_arg_display_len);
        println!("{}: {}", "Arguments".cyan(), args_display);
    }

    println!("{}", "═══════════════════════".yellow().bold());
}

/// Display multiple previews for a batch of tool calls
pub fn display_batch_preview(tool_calls: &[ParsedToolCall], config: &DryRunConfig) {
    println!();
    println!(
        "{}",
        "╔═══════════════════════════════════════╗".yellow().bold()
    );
    println!(
        "{}",
        "║       DRY RUN - PLANNED ACTIONS       ║".yellow().bold()
    );
    println!(
        "{}",
        "╚═══════════════════════════════════════╝".yellow().bold()
    );
    println!();

    for (i, call) in tool_calls.iter().enumerate() {
        let preview = preview_tool_call(&call.tool_name, &call.arguments, config);

        println!(
            "{}. {} - {}",
            (i + 1).to_string().white().bold(),
            preview.tool_name.green(),
            preview.description
        );

        if !preview.would_modify.is_empty() {
            println!(
                "   {} {}",
                "→".yellow(),
                preview.would_modify.join(", ").dimmed()
            );
        }

        println!(
            "   {} {}",
            "⚠".yellow(),
            colorize_risk(&preview.risk_assessment)
        );
        println!();
    }

    println!("{}", "─".repeat(40).dimmed());
    println!(
        "Total operations: {} | Run without --dry-run to execute",
        tool_calls.len()
    );
}

/// Colorize risk assessment text
fn colorize_risk(risk: &str) -> colored::ColoredString {
    if risk.contains("HIGH") {
        risk.red()
    } else if risk.contains("MEDIUM") {
        risk.yellow()
    } else if risk.contains("Safe") {
        risk.green()
    } else {
        risk.normal()
    }
}

/// Truncate a string for display
fn truncate_str(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        let end = s.floor_char_boundary(max_len.saturating_sub(3));
        format!("{}...", &s[..end])
    }
}

#[cfg(test)]
#[path = "../../tests/unit/safety/dry_run/dry_run_test.rs"]
mod tests;
