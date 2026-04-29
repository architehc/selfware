//! Shell execution tool - runs shell commands with timeout and safety checks.

use crate::tools::file::{is_file_stale, write_atomic};
use crate::tools::Tool;
use anyhow::Result;
use async_trait::async_trait;
use serde::Deserialize;
use serde_json::Value;
use std::collections::HashMap;
use std::path::Path;
use std::time::Duration;

pub mod prompt;

/// Attempt to parse a straightforward `sed -i 's/old/new/g' file` substitution.
/// Returns `(file, old_str, new_str, global)` if the pattern is simple enough
/// to intercept, or `None` for complex cases that should run raw.
fn try_parse_sed_substitution(command: &str) -> Option<(String, String, String, bool)> {
    let trimmed = command.trim();

    // Must start with sed and contain -i or --in-place
    if !trimmed.starts_with("sed") {
        return None;
    }
    if !trimmed.contains("-i") && !trimmed.contains("--in-place") {
        return None;
    }

    // Find the quoted substitution expression: look for 's... or "s...
    let (quote_start, quote_char) = trimmed
        .char_indices()
        .find(|(_, c)| *c == '\'' || *c == '"')
        .and_then(|(i, c)| trimmed.get(i + 1..)?.starts_with('s').then_some((i, c)))?;

    let after_quote = &trimmed[quote_start + 1..];
    let sub_end = after_quote.find(quote_char)?;
    let sub_expr = &after_quote[..sub_end]; // e.g. "s/old/new/g"

    if sub_expr.len() < 5 {
        return None;
    }

    let delimiter = sub_expr.chars().nth(1)?;
    let rest = &sub_expr[2..];
    let parts: Vec<&str> = rest.split(delimiter).collect();
    if parts.len() < 2 {
        return None;
    }

    let old_str = parts[0].to_string();
    let new_str = parts[1].to_string();
    let flags = if parts.len() > 2 { parts[2] } else { "" };
    let global = flags.contains('g') || flags.contains('G');

    // Only intercept simple literal patterns (no regex metacharacters)
    if old_str.contains([
        '.', '*', '+', '?', '^', '$', '[', ']', '(', ')', '{', '}', '|', '\\',
    ]) {
        return None;
    }

    // Extract filename: text after the closing quote, trimmed
    let after_sub = &after_quote[sub_end + 1..].trim();
    if after_sub.is_empty() {
        return None;
    }
    let file_tokens: Vec<&str> = after_sub.split_whitespace().collect();
    if file_tokens.len() != 1 {
        return None; // Multiple files or extra flags — too complex
    }
    let file = file_tokens[0].to_string();
    if file.starts_with('-') {
        return None;
    }

    Some((file, old_str, new_str, global))
}

/// Apply a sed-like literal substitution with stale-guard protection.
async fn apply_sed_substitution(
    file: &str,
    old_str: &str,
    new_str: &str,
    global: bool,
) -> anyhow::Result<Value> {
    // Path validation
    let safety = crate::tools::file::resolve_safety_config(None);
    crate::tools::file::validate_tool_path(file, &safety)
        .map_err(|e| anyhow::anyhow!("sed interception path validation failed: {}", e))?;

    // Stale-guard
    if let Some(true) = is_file_stale(file) {
        anyhow::bail!(
            "File {} changed on disk since you last read it. Re-read the file and try again.",
            file
        );
    }

    let content = tokio::fs::read_to_string(file).await?;
    let new_content = if global {
        content.replace(old_str, new_str)
    } else {
        content.replacen(old_str, new_str, 1)
    };

    if new_content == content {
        anyhow::bail!(
            "sed substitution had no effect — old_str not found in {}",
            file
        );
    }

    write_atomic(Path::new(file), &new_content).await?;

    Ok(serde_json::json!({
        "exit_code": 0,
        "stdout": "",
        "stderr": "",
        "stdout_pagination": {"offset":0,"limit":10000,"total_chars":0,"has_more":false},
        "stderr_pagination": {"offset":0,"limit":10000,"total_chars":0,"has_more":false},
        "duration_ms": 0,
        "timed_out": false,
        "intercepted": true,
        "tool": "file_edit"
    }))
}

/// Returns the platform-appropriate shell and flag for command execution.
///
/// On Windows, returns `("cmd", "/C")`. On Unix-like systems, returns `("sh", "-c")`.
pub fn default_shell() -> (&'static str, &'static str) {
    if cfg!(target_os = "windows") {
        ("cmd", "/C")
    } else {
        ("sh", "-c")
    }
}

/// Shell command execution tool.
pub struct ShellExec;

#[async_trait]
impl Tool for ShellExec {
    fn name(&self) -> &str {
        "shell_exec"
    }

    fn description(&self) -> &str {
        "Execute shell command. Use for builds, tests, and system operations. Runs with timeout."
    }

    fn schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "command": {"type": "string", "description": "Command to execute"},
                "cwd": {"type": "string", "description": "Working directory"},
                "timeout_secs": {"type": "integer", "default": 60, "description": "Timeout in seconds"},
                "env": {"type": "object", "additionalProperties": {"type": "string"}},
                "output_offset": {"type": "integer", "default": 0, "description": "Character offset for paginated output"},
                "output_limit": {"type": "integer", "default": 10000, "description": "Maximum characters per output page"}
            },
            "required": ["command"]
        })
    }

    async fn execute(&self, args: Value) -> Result<Value> {
        #[derive(Deserialize)]
        struct Args {
            command: String,
            cwd: Option<String>,
            #[serde(default = "default_timeout")]
            timeout_secs: u64,
            #[serde(default)]
            env: HashMap<String, String>,
            #[serde(default)]
            output_offset: usize,
            #[serde(default = "default_output_limit")]
            output_limit: usize,
        }

        fn default_timeout() -> u64 {
            60
        }

        fn default_output_limit() -> usize {
            10000
        }

        let mut args: Args = serde_json::from_value(args)?;

        // Cap timeout to prevent indefinite hangs (1 hour max)
        const MAX_TIMEOUT_SECS: u64 = 3600;
        args.timeout_secs = args.timeout_secs.min(MAX_TIMEOUT_SECS);

        // Command length limit to prevent abuse
        const MAX_COMMAND_LENGTH: usize = 10_000;
        if args.command.len() > MAX_COMMAND_LENGTH {
            anyhow::bail!(
                "Command exceeds maximum length of {} characters",
                MAX_COMMAND_LENGTH
            );
        }

        // Block dangerous patterns that are common in reverse shells and
        // data exfiltration payloads. This is defense-in-depth; the safety
        // checker provides the primary validation layer.
        if let Some(pattern) = super::find_dangerous_shell_pattern(&args.command) {
            anyhow::bail!("Blocked potentially dangerous shell pattern: {}", pattern);
        }

        // Validate cwd: must be an absolute path without path traversal components
        if let Some(cwd) = &args.cwd {
            let cwd_path = Path::new(cwd);
            if !cwd_path.is_absolute() {
                anyhow::bail!("cwd must be an absolute path, got: {}", cwd);
            }
            for component in cwd_path.components() {
                if let std::path::Component::ParentDir = component {
                    anyhow::bail!("cwd must not contain path traversal (..): {}", cwd);
                }
            }
        }

        // Validate environment variable names and values
        for (name, value) in &args.env {
            if name.contains('=') {
                anyhow::bail!("Environment variable name must not contain '=': {}", name);
            }
            if name.contains('\0') {
                anyhow::bail!(
                    "Environment variable name must not contain null bytes: {}",
                    name
                );
            }
            if value.contains('\0') {
                anyhow::bail!(
                    "Environment variable value must not contain null bytes (var: {})",
                    name
                );
            }
        }

        // Intercept simple sed -i substitutions and route through file tools
        // for stale-guard protection
        if let Some((file, old_str, new_str, global)) = try_parse_sed_substitution(&args.command) {
            return apply_sed_substitution(&file, &old_str, &new_str, global).await;
        }

        let (shell, flag) = default_shell();
        let mut cmd = tokio::process::Command::new(shell);
        cmd.kill_on_drop(true);
        cmd.arg(flag).arg(&args.command);

        if let Some(cwd) = &args.cwd {
            cmd.current_dir(cwd);
        }

        // Clear inherited environment to prevent secret leakage, then set a minimal base
        cmd.env_clear();
        if let Ok(path) = std::env::var("PATH") {
            cmd.env("PATH", path);
        }
        if let Ok(home) = std::env::var("HOME") {
            cmd.env("HOME", home);
        }
        if let Ok(lang) = std::env::var("LANG") {
            cmd.env("LANG", lang);
        }
        cmd.envs(&args.env);

        let start = std::time::Instant::now();
        let output =
            tokio::time::timeout(Duration::from_secs(args.timeout_secs), cmd.output()).await;

        let (exit_code, stdout, stderr, timed_out) = match output {
            Ok(Ok(output)) => (
                output.status.code().unwrap_or(-1),
                String::from_utf8_lossy(&output.stdout).into_owned(),
                String::from_utf8_lossy(&output.stderr).into_owned(),
                false,
            ),
            Ok(Err(e)) => return Err(e.into()),
            Err(_) => (-1, "".to_string(), "Command timed out".to_string(), true),
        };

        let duration_ms = start.elapsed().as_millis() as u64;

        let (stdout_page, stdout_pagination) =
            super::truncate_with_pagination(&stdout, args.output_offset, args.output_limit);
        let (stderr_page, stderr_pagination) =
            super::truncate_with_pagination(&stderr, args.output_offset, args.output_limit);

        Ok(serde_json::json!({
            "exit_code": exit_code,
            "stdout": stdout_page,
            "stderr": stderr_page,
            "stdout_pagination": stdout_pagination,
            "stderr_pagination": stderr_pagination,
            "duration_ms": duration_ms,
            "timed_out": timed_out
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_shell_exec_name() {
        let tool = ShellExec;
        assert_eq!(tool.name(), "shell_exec");
    }

    #[test]
    fn test_shell_exec_description() {
        let tool = ShellExec;
        assert!(tool.description().contains("Execute"));
        assert!(tool.description().contains("command"));
    }

    #[test]
    fn test_shell_exec_schema() {
        let tool = ShellExec;
        let schema = tool.schema();
        assert_eq!(schema["type"], "object");
        assert!(schema["properties"]["command"].is_object());
        assert!(schema["properties"]["timeout_secs"].is_object());
    }

    #[tokio::test]
    async fn test_shell_exec_echo() {
        let tool = ShellExec;
        let args = serde_json::json!({
            "command": "echo 'hello world'",
            "timeout_secs": 5
        });

        let result = tool.execute(args).await.unwrap();
        assert_eq!(result["exit_code"], 0);
        assert!(result["stdout"].as_str().unwrap().contains("hello world"));
        assert_eq!(result["timed_out"], false);
    }

    #[tokio::test]
    async fn test_shell_exec_exit_code() {
        let tool = ShellExec;
        let args = serde_json::json!({
            "command": "exit 42",
            "timeout_secs": 5
        });

        let result = tool.execute(args).await.unwrap();
        assert_eq!(result["exit_code"], 42);
    }

    #[tokio::test]
    async fn test_shell_exec_stderr() {
        let tool = ShellExec;
        let args = serde_json::json!({
            "command": "echo 'error' >&2",
            "timeout_secs": 5
        });

        let result = tool.execute(args).await.unwrap();
        assert!(result["stderr"].as_str().unwrap().contains("error"));
    }

    #[tokio::test]
    async fn test_shell_exec_with_env() {
        let tool = ShellExec;
        let command = if cfg!(target_os = "windows") {
            "echo %MY_VAR%"
        } else {
            "echo $MY_VAR"
        };
        let args = serde_json::json!({
            "command": command,
            "timeout_secs": 5,
            "env": {
                "MY_VAR": "custom_value"
            }
        });

        let result = tool.execute(args).await.unwrap();
        assert!(result["stdout"].as_str().unwrap().contains("custom_value"));
    }

    #[tokio::test]
    #[cfg(not(target_os = "windows"))]
    async fn test_shell_exec_with_cwd() {
        let tool = ShellExec;
        let args = serde_json::json!({
            "command": "pwd",
            "cwd": "/tmp",
            "timeout_secs": 5
        });

        let result = tool.execute(args).await.unwrap();
        assert!(result["stdout"].as_str().unwrap().contains("/tmp"));
    }

    #[tokio::test]
    async fn test_shell_exec_timeout() {
        let tool = ShellExec;
        let args = serde_json::json!({
            "command": "sleep 10",
            "timeout_secs": 1
        });

        let result = tool.execute(args).await.unwrap();
        assert_eq!(result["timed_out"], true);
        assert!(result["stderr"].as_str().unwrap().contains("timed out"));
    }

    #[tokio::test]
    async fn test_dangerous_pattern_dev_tcp() {
        let tool = ShellExec;
        let args = serde_json::json!({
            "command": "cat < /dev/tcp/127.0.0.1/8080",
            "timeout_secs": 5
        });
        let result = tool.execute(args).await;
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("Blocked potentially dangerous shell pattern"));
    }

    #[tokio::test]
    async fn test_cwd_relative_path_rejected() {
        let tool = ShellExec;
        let args = serde_json::json!({
            "command": "echo test",
            "cwd": "relative/path",
            "timeout_secs": 5
        });
        let result = tool.execute(args).await;
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("cwd must be an absolute path"));
    }

    #[tokio::test]
    async fn test_command_exceeds_max_length_rejected() {
        let tool = ShellExec;
        let long_cmd = "a".repeat(10_001);
        let args = serde_json::json!({
            "command": long_cmd,
            "timeout_secs": 5
        });
        let result = tool.execute(args).await;
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("exceeds maximum length"));
    }

    // --- sed -i interception tests ---

    #[test]
    fn test_parse_sed_substitution_simple() {
        let result = try_parse_sed_substitution("sed -i 's/old/new/g' file.txt");
        assert!(result.is_some());
        let (file, old, new, global) = result.unwrap();
        assert_eq!(file, "file.txt");
        assert_eq!(old, "old");
        assert_eq!(new, "new");
        assert!(global);
    }

    #[test]
    fn test_parse_sed_substitution_no_global() {
        let result = try_parse_sed_substitution("sed -i 's/old/new/' file.txt");
        assert!(result.is_some());
        let (_, _, _, global) = result.unwrap();
        assert!(!global);
    }

    #[test]
    fn test_parse_sed_substitution_falls_back_for_complex() {
        // Regex metacharacters in pattern
        assert!(try_parse_sed_substitution("sed -i 's/foo.bar/baz/g' file.txt").is_none());
        // Multiple files
        assert!(try_parse_sed_substitution("sed -i 's/a/b/g' f1 f2").is_none());
        // Not sed
        assert!(try_parse_sed_substitution("echo hello").is_none());
    }

    #[tokio::test]
    #[cfg(not(target_os = "windows"))]
    async fn test_shell_exec_intercepts_sed() {
        let tool = ShellExec;
        let temp_dir =
            std::env::temp_dir().join(format!("selfware-sed-test-{}", std::process::id()));
        tokio::fs::create_dir_all(&temp_dir).await.unwrap();
        let file_path = temp_dir.join("test.txt");
        tokio::fs::write(&file_path, "hello world\nfoo bar\n")
            .await
            .unwrap();

        let args = serde_json::json!({
            "command": format!("sed -i 's/foo/FOO/g' {}", file_path.display()),
            "timeout_secs": 5
        });

        let result = tool.execute(args).await.unwrap();
        assert_eq!(result["exit_code"], 0);
        assert_eq!(result["intercepted"], true);

        let content = tokio::fs::read_to_string(&file_path).await.unwrap();
        assert!(content.contains("FOO bar"));
        assert!(!content.contains("foo bar"));

        let _ = tokio::fs::remove_dir_all(&temp_dir).await;
    }
}
