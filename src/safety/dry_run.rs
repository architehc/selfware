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
mod tests {
    use super::*;

    #[test]
    fn test_dry_run_config_default() {
        let config = DryRunConfig::default();
        assert!(!config.enabled);
        assert!(config.show_arguments);
    }

    #[test]
    fn test_preview_file_read() {
        let config = DryRunConfig::default();
        let args = serde_json::json!({"path": "test.txt"});
        let preview = preview_tool_call("file_read", &args, &config);

        assert_eq!(preview.tool_name, "file_read");
        assert!(preview.description.contains("test.txt"));
        assert!(preview.would_modify.is_empty());
        assert!(preview.risk_assessment.contains("Safe"));
    }

    #[test]
    fn test_preview_file_write() {
        let config = DryRunConfig::default();
        let args = serde_json::json!({"path": "output.txt", "content": "hello"});
        let preview = preview_tool_call("file_write", &args, &config);

        assert!(preview.would_modify.contains(&"output.txt".to_string()));
        assert!(preview.risk_assessment.contains("Modifies"));
    }

    #[test]
    fn test_preview_shell_exec_dangerous() {
        let config = DryRunConfig::default();
        let args = serde_json::json!({"command": "rm -rf /tmp/test"});
        let preview = preview_tool_call("shell_exec", &args, &config);

        assert!(preview.risk_assessment.contains("HIGH"));
    }

    #[test]
    fn test_preview_shell_exec_safe() {
        let config = DryRunConfig::default();
        let args = serde_json::json!({"command": "ls -la"});
        let preview = preview_tool_call("shell_exec", &args, &config);

        assert!(!preview.risk_assessment.contains("HIGH"));
    }

    #[test]
    fn test_truncate_str_short() {
        assert_eq!(truncate_str("hello", 10), "hello");
    }

    #[test]
    fn test_truncate_str_long() {
        assert_eq!(truncate_str("hello world", 5), "hello...");
    }

    #[test]
    fn test_preview_git_force_push() {
        let config = DryRunConfig::default();
        let args = serde_json::json!({"force": true});
        let preview = preview_tool_call("git_push", &args, &config);

        assert!(preview.risk_assessment.contains("HIGH"));
        assert!(preview.description.contains("--force"));
    }

    #[test]
    fn test_preview_unknown_tool() {
        let config = DryRunConfig::default();
        let args = serde_json::json!({});
        let preview = preview_tool_call("custom_tool", &args, &config);

        assert!(preview.description.contains("custom_tool"));
        assert!(preview.risk_assessment.contains("Unknown"));
    }

    #[test]
    fn test_preview_file_edit() {
        let config = DryRunConfig::default();
        let args = serde_json::json!({
            "path": "src/main.rs",
            "old_str": "hello",
            "new_str": "world"
        });
        let preview = preview_tool_call("file_edit", &args, &config);

        assert!(preview.description.contains("src/main.rs"));
        assert!(preview.would_modify.contains(&"src/main.rs".to_string()));
        assert!(preview.risk_assessment.contains("Modifies"));
    }

    #[test]
    fn test_preview_directory_tree() {
        let config = DryRunConfig::default();
        let args = serde_json::json!({"path": "/home/user"});
        let preview = preview_tool_call("directory_tree", &args, &config);

        assert!(preview.description.contains("/home/user"));
        assert!(preview.would_modify.is_empty());
        assert!(preview.risk_assessment.contains("Safe"));
    }

    #[test]
    fn test_preview_git_commit() {
        let config = DryRunConfig::default();
        let args = serde_json::json!({"message": "Fix bug in parser"});
        let preview = preview_tool_call("git_commit", &args, &config);

        assert!(preview.description.contains("Fix bug"));
        assert!(preview.would_modify.contains(&".git/".to_string()));
        assert!(preview.risk_assessment.contains("Safe"));
    }

    #[test]
    fn test_preview_git_push_normal() {
        let config = DryRunConfig::default();
        let args = serde_json::json!({"force": false});
        let preview = preview_tool_call("git_push", &args, &config);

        assert!(!preview.description.contains("--force"));
        assert!(preview.risk_assessment.contains("MEDIUM"));
    }

    #[test]
    fn test_preview_cargo_test() {
        let config = DryRunConfig::default();
        let args = serde_json::json!({});
        let preview = preview_tool_call("cargo_test", &args, &config);

        assert!(preview.description.contains("cargo test"));
        assert!(preview.risk_assessment.contains("Safe"));
    }

    #[test]
    fn test_preview_cargo_check() {
        let config = DryRunConfig::default();
        let args = serde_json::json!({});
        let preview = preview_tool_call("cargo_check", &args, &config);

        assert!(preview.description.contains("cargo check"));
        assert!(preview.risk_assessment.contains("Safe"));
    }

    #[test]
    fn test_preview_cargo_clippy() {
        let config = DryRunConfig::default();
        let args = serde_json::json!({});
        let preview = preview_tool_call("cargo_clippy", &args, &config);

        assert!(preview.description.contains("cargo clippy"));
        assert!(preview.risk_assessment.contains("Safe"));
    }

    #[test]
    fn test_preview_http_request() {
        let config = DryRunConfig::default();
        let args = serde_json::json!({
            "url": "https://api.example.com/data",
            "method": "POST"
        });
        let preview = preview_tool_call("http_request", &args, &config);

        assert!(preview.description.contains("POST"));
        assert!(preview.description.contains("api.example.com"));
        assert!(preview.risk_assessment.contains("Network"));
    }

    #[test]
    fn test_preview_grep_search() {
        let config = DryRunConfig::default();
        let args = serde_json::json!({"pattern": "TODO"});
        let preview = preview_tool_call("grep_search", &args, &config);

        assert!(preview.description.contains("TODO"));
        assert!(preview.risk_assessment.contains("Safe"));
    }

    #[test]
    fn test_preview_glob_find() {
        let config = DryRunConfig::default();
        let args = serde_json::json!({"pattern": "*.rs"});
        let preview = preview_tool_call("glob_find", &args, &config);

        assert!(preview.description.contains("*.rs"));
        assert!(preview.risk_assessment.contains("Safe"));
    }

    #[test]
    fn test_preview_symbol_search() {
        let config = DryRunConfig::default();
        let args = serde_json::json!({"pattern": "main"});
        let preview = preview_tool_call("symbol_search", &args, &config);

        assert!(preview.description.contains("main"));
        assert!(preview.risk_assessment.contains("Safe"));
    }

    #[test]
    fn test_preview_shell_exec_medium_risk() {
        let config = DryRunConfig::default();
        let args = serde_json::json!({"command": "mv old.txt new.txt"});
        let preview = preview_tool_call("shell_exec", &args, &config);

        assert!(preview.risk_assessment.contains("MEDIUM"));
    }

    #[test]
    fn test_preview_with_arguments_hidden() {
        let config = DryRunConfig {
            show_arguments: false,
            ..Default::default()
        };
        let args = serde_json::json!({"path": "secret.txt"});
        let preview = preview_tool_call("file_read", &args, &config);

        assert_eq!(preview.arguments, serde_json::Value::Null);
    }

    #[test]
    fn test_colorize_risk_high() {
        let colored = colorize_risk("HIGH - dangerous");
        assert!(colored.to_string().contains("HIGH"));
    }

    #[test]
    fn test_colorize_risk_medium() {
        let colored = colorize_risk("MEDIUM - caution");
        assert!(colored.to_string().contains("MEDIUM"));
    }

    #[test]
    fn test_colorize_risk_safe() {
        let colored = colorize_risk("Safe - read only");
        assert!(colored.to_string().contains("Safe"));
    }

    #[test]
    fn test_colorize_risk_unknown() {
        let colored = colorize_risk("Unknown risk level");
        assert!(colored.to_string().contains("Unknown"));
    }

    #[test]
    fn test_dry_run_preview_struct() {
        let preview = DryRunPreview {
            tool_name: "test_tool".to_string(),
            description: "Test description".to_string(),
            arguments: serde_json::json!({"key": "value"}),
            would_modify: vec!["file.txt".to_string()],
            risk_assessment: "Safe".to_string(),
        };

        assert_eq!(preview.tool_name, "test_tool");
        assert_eq!(preview.would_modify.len(), 1);
    }

    #[test]
    fn test_dry_run_config_custom() {
        let config = DryRunConfig {
            enabled: true,
            show_arguments: false,
            show_diff_preview: false,
            max_arg_display_len: 50,
        };

        assert!(config.enabled);
        assert!(!config.show_arguments);
        assert!(!config.show_diff_preview);
        assert_eq!(config.max_arg_display_len, 50);
    }

    #[test]
    fn test_dry_run_config_clone() {
        let config = DryRunConfig::default();
        let cloned = config.clone();

        assert_eq!(config.enabled, cloned.enabled);
        assert_eq!(config.show_arguments, cloned.show_arguments);
    }

    #[test]
    fn test_dry_run_config_debug() {
        let config = DryRunConfig::default();
        let debug_str = format!("{:?}", config);

        assert!(debug_str.contains("DryRunConfig"));
        assert!(debug_str.contains("enabled"));
    }

    #[test]
    fn test_dry_run_preview_clone() {
        let preview = DryRunPreview {
            tool_name: "test".to_string(),
            description: "desc".to_string(),
            arguments: serde_json::json!({}),
            would_modify: vec!["file.txt".to_string()],
            risk_assessment: "Safe".to_string(),
        };

        let cloned = preview.clone();
        assert_eq!(preview.tool_name, cloned.tool_name);
        assert_eq!(preview.would_modify, cloned.would_modify);
    }

    #[test]
    fn test_dry_run_preview_debug() {
        let preview = DryRunPreview {
            tool_name: "debug_test".to_string(),
            description: "testing debug".to_string(),
            arguments: serde_json::json!({"key": "value"}),
            would_modify: vec![],
            risk_assessment: "Low".to_string(),
        };

        let debug_str = format!("{:?}", preview);
        assert!(debug_str.contains("DryRunPreview"));
        assert!(debug_str.contains("debug_test"));
    }

    #[test]
    fn test_preview_file_read_missing_path() {
        let config = DryRunConfig::default();
        let args = serde_json::json!({});
        let preview = preview_tool_call("file_read", &args, &config);

        assert!(preview.description.contains("?"));
    }

    #[test]
    fn test_preview_file_write_missing_content() {
        let config = DryRunConfig::default();
        let args = serde_json::json!({"path": "file.txt"});
        let preview = preview_tool_call("file_write", &args, &config);

        assert!(preview.description.contains("0 bytes"));
    }

    #[test]
    fn test_preview_file_edit_missing_strings() {
        let config = DryRunConfig::default();
        let args = serde_json::json!({"path": "file.txt"});
        let preview = preview_tool_call("file_edit", &args, &config);

        assert!(preview.description.contains("0 chars"));
    }

    #[test]
    fn test_preview_directory_tree_default_path() {
        let config = DryRunConfig::default();
        let args = serde_json::json!({});
        let preview = preview_tool_call("directory_tree", &args, &config);

        assert!(preview.description.contains("."));
    }

    #[test]
    fn test_preview_shell_exec_with_redirect() {
        let config = DryRunConfig::default();
        let args = serde_json::json!({"command": "echo hello > output.txt"});
        let preview = preview_tool_call("shell_exec", &args, &config);

        assert!(preview.risk_assessment.contains("MEDIUM"));
    }

    #[test]
    fn test_preview_shell_exec_with_cp() {
        let config = DryRunConfig::default();
        let args = serde_json::json!({"command": "cp file1.txt file2.txt"});
        let preview = preview_tool_call("shell_exec", &args, &config);

        assert!(preview.risk_assessment.contains("MEDIUM"));
    }

    #[test]
    fn test_preview_shell_exec_with_delete() {
        let config = DryRunConfig::default();
        let args = serde_json::json!({"command": "delete file.txt"});
        let preview = preview_tool_call("shell_exec", &args, &config);

        assert!(preview.risk_assessment.contains("HIGH"));
    }

    #[test]
    fn test_preview_git_commit_long_message() {
        let config = DryRunConfig::default();
        let long_msg = "A".repeat(100);
        let args = serde_json::json!({"message": long_msg});
        let preview = preview_tool_call("git_commit", &args, &config);

        // Message should be truncated
        assert!(preview.description.len() < 150);
    }

    #[test]
    fn test_preview_http_request_get() {
        let config = DryRunConfig::default();
        let args = serde_json::json!({"url": "https://example.com"});
        let preview = preview_tool_call("http_request", &args, &config);

        assert!(preview.description.contains("GET"));
    }

    #[test]
    fn test_truncate_str_exact_length() {
        assert_eq!(truncate_str("hello", 5), "hello");
    }

    #[test]
    fn test_truncate_str_empty() {
        assert_eq!(truncate_str("", 10), "");
    }

    #[test]
    fn test_truncate_str_unicode() {
        // Note: truncate_str may not handle unicode boundaries well
        let result = truncate_str("hello世界", 5);
        assert!(result.starts_with("hello"));
    }

    #[test]
    fn test_preview_with_complex_arguments() {
        let config = DryRunConfig::default();
        let args = serde_json::json!({
            "path": "complex.txt",
            "content": "multi\nline\ncontent",
            "nested": {
                "key": "value",
                "array": [1, 2, 3]
            }
        });
        let preview = preview_tool_call("file_write", &args, &config);

        assert!(preview.arguments.is_object());
    }

    #[test]
    fn test_display_preview_does_not_panic() {
        let config = DryRunConfig::default();
        let preview = DryRunPreview {
            tool_name: "test".to_string(),
            description: "Test".to_string(),
            arguments: serde_json::json!({"key": "value"}),
            would_modify: vec!["file.txt".to_string()],
            risk_assessment: "Safe".to_string(),
        };

        // Just verify it doesn't panic
        display_preview(&preview, &config);
    }

    #[test]
    fn test_display_preview_empty_would_modify() {
        let config = DryRunConfig::default();
        let preview = DryRunPreview {
            tool_name: "test".to_string(),
            description: "Read-only".to_string(),
            arguments: serde_json::json!({}),
            would_modify: vec![],
            risk_assessment: "Safe".to_string(),
        };

        display_preview(&preview, &config);
    }

    #[test]
    fn test_display_preview_arguments_hidden() {
        let config = DryRunConfig {
            show_arguments: false,
            ..Default::default()
        };
        let preview = DryRunPreview {
            tool_name: "test".to_string(),
            description: "Test".to_string(),
            arguments: serde_json::Value::Null,
            would_modify: vec![],
            risk_assessment: "Safe".to_string(),
        };

        display_preview(&preview, &config);
    }

    #[test]
    fn test_preview_git_push_without_force() {
        let config = DryRunConfig::default();
        let args = serde_json::json!({});
        let preview = preview_tool_call("git_push", &args, &config);

        assert!(!preview.description.contains("--force"));
    }

    #[test]
    fn test_colorize_risk_returns_colored_string() {
        let high = colorize_risk("HIGH risk");
        let medium = colorize_risk("MEDIUM risk");
        let safe = colorize_risk("Safe operation");
        let other = colorize_risk("Other");

        // All should be ColoredString
        assert!(!high.to_string().is_empty());
        assert!(!medium.to_string().is_empty());
        assert!(!safe.to_string().is_empty());
        assert!(!other.to_string().is_empty());
    }

    #[test]
    fn test_preview_multiple_tools() {
        let config = DryRunConfig::default();

        let tools = vec![
            ("file_read", serde_json::json!({"path": "test.txt"})),
            (
                "file_write",
                serde_json::json!({"path": "out.txt", "content": "data"}),
            ),
            ("shell_exec", serde_json::json!({"command": "ls"})),
            ("git_commit", serde_json::json!({"message": "test"})),
            ("cargo_test", serde_json::json!({})),
        ];

        for (tool_name, args) in tools {
            let preview = preview_tool_call(tool_name, &args, &config);
            assert!(!preview.description.is_empty());
            assert!(!preview.risk_assessment.is_empty());
        }
    }

    #[test]
    fn test_preview_file_write_large_content() {
        let config = DryRunConfig::default();
        let large_content = "x".repeat(10000);
        let args = serde_json::json!({"path": "large.txt", "content": large_content});
        let preview = preview_tool_call("file_write", &args, &config);

        assert!(preview.description.contains("10000 bytes"));
    }

    #[test]
    fn test_dry_run_preview_multiple_modifications() {
        let preview = DryRunPreview {
            tool_name: "batch_tool".to_string(),
            description: "Batch operation".to_string(),
            arguments: serde_json::json!({}),
            would_modify: vec![
                "file1.txt".to_string(),
                "file2.txt".to_string(),
                "file3.txt".to_string(),
            ],
            risk_assessment: "MEDIUM".to_string(),
        };

        assert_eq!(preview.would_modify.len(), 3);
    }

    #[test]
    fn test_preview_shell_exec_truncation() {
        let config = DryRunConfig::default();
        let long_cmd = "echo ".to_string() + &"a".repeat(100);
        let args = serde_json::json!({"command": long_cmd});
        let preview = preview_tool_call("shell_exec", &args, &config);

        // Description should be truncated
        assert!(preview.description.len() < 100);
    }

    #[test]
    fn test_preview_http_request_long_url() {
        let config = DryRunConfig::default();
        let long_url = "https://example.com/".to_string() + &"path/".repeat(20);
        let args = serde_json::json!({"url": long_url, "method": "GET"});
        let preview = preview_tool_call("http_request", &args, &config);

        // URL should be truncated in description
        assert!(preview.description.len() < 100);
    }

    #[test]
    fn test_display_batch_preview_does_not_panic() {
        use crate::tool_parser::{ParseMethod, ParsedToolCall};

        let config = DryRunConfig::default();
        let tool_calls = vec![
            ParsedToolCall {
                tool_name: "file_read".to_string(),
                arguments: serde_json::json!({"path": "test.txt"}),
                raw_text: "{}".to_string(),
                parse_method: ParseMethod::Json,
            },
            ParsedToolCall {
                tool_name: "file_write".to_string(),
                arguments: serde_json::json!({"path": "out.txt", "content": "test"}),
                raw_text: "{}".to_string(),
                parse_method: ParseMethod::Json,
            },
        ];

        // Should not panic
        display_batch_preview(&tool_calls, &config);
    }

    #[test]
    fn test_display_batch_preview_empty() {
        use crate::tool_parser::ParsedToolCall;

        let config = DryRunConfig::default();
        let tool_calls: Vec<ParsedToolCall> = vec![];

        // Should handle empty list without panic
        display_batch_preview(&tool_calls, &config);
    }
}
