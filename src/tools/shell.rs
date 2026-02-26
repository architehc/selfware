use super::Tool;
use anyhow::Result;
use async_trait::async_trait;
use serde::Deserialize;
use serde_json::Value;
use std::collections::HashMap;
use std::path::Path;
use std::time::Duration;

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
                "env": {"type": "object", "additionalProperties": {"type": "string"}}
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
        }

        fn default_timeout() -> u64 {
            60
        }

        let args: Args = serde_json::from_value(args)?;

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
        let lower_cmd = args.command.to_lowercase();
        let dangerous_patterns: &[&str] = &[
            "/dev/tcp/",
            "/dev/udp/",
            "| bash -i",
            "| sh -i",
            "mkfifo /tmp",
        ];
        for pattern in dangerous_patterns {
            if lower_cmd.contains(pattern) {
                anyhow::bail!("Blocked potentially dangerous shell pattern: {}", pattern);
            }
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

        let (shell, flag) = default_shell();
        let mut cmd = tokio::process::Command::new(shell);
        cmd.kill_on_drop(true);
        cmd.arg(flag).arg(&args.command);

        if let Some(cwd) = &args.cwd {
            cmd.current_dir(cwd);
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

        Ok(serde_json::json!({
            "exit_code": exit_code,
            "stdout": stdout.chars().take(10000).collect::<String>(),
            "stderr": stderr.chars().take(10000).collect::<String>(),
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
        let args = serde_json::json!({
            "command": "echo $MY_VAR",
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
    async fn test_shell_exec_duration_tracked() {
        let tool = ShellExec;
        let args = serde_json::json!({
            "command": "sleep 0.1",
            "timeout_secs": 5
        });

        let result = tool.execute(args).await.unwrap();
        let duration = result["duration_ms"].as_u64().unwrap();
        assert!(duration >= 50); // At least 50ms
    }

    #[tokio::test]
    async fn test_shell_exec_truncates_long_output() {
        let tool = ShellExec;
        // Generate a lot of output
        let args = serde_json::json!({
            "command": "yes | head -n 100000",
            "timeout_secs": 10
        });

        let result = tool.execute(args).await.unwrap();
        let stdout = result["stdout"].as_str().unwrap();
        // Should be truncated to 10000 chars
        assert!(stdout.len() <= 10000);
    }

    #[tokio::test]
    async fn test_shell_exec_default_timeout() {
        let tool = ShellExec;
        // No timeout specified, should use default
        let args = serde_json::json!({
            "command": "echo 'quick'"
        });

        let result = tool.execute(args).await.unwrap();
        assert_eq!(result["exit_code"], 0);
    }

    #[tokio::test]
    async fn test_shell_exec_complex_command() {
        let tool = ShellExec;
        let args = serde_json::json!({
            "command": "echo 'a' && echo 'b' && echo 'c'",
            "timeout_secs": 5
        });

        let result = tool.execute(args).await.unwrap();
        let stdout = result["stdout"].as_str().unwrap();
        assert!(stdout.contains("a"));
        assert!(stdout.contains("b"));
        assert!(stdout.contains("c"));
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
    async fn test_shell_exec_empty_env() {
        let tool = ShellExec;
        let args = serde_json::json!({
            "command": "echo test",
            "env": {}
        });

        let result = tool.execute(args).await.unwrap();
        assert_eq!(result["exit_code"], 0);
    }
}
