//! Safety Layer - Tool Call Validation
//!
//! Validates tool calls before execution to prevent dangerous operations.
//! Checks include:
//! - Path traversal prevention (no escaping allowed directories)
//! - Protected path enforcement (no modifications to system directories)
//! - Command blacklisting for shell operations
//! - Configurable per-tool safety rules
//!
//! This is the first line of defense; YOLO mode provides additional controls.

use crate::api::types::ToolCall;
use crate::config::SafetyConfig;
use anyhow::Result;
use std::path::{Path, PathBuf};

pub struct SafetyChecker {
    config: SafetyConfig,
    /// Working directory for resolving relative paths
    working_dir: PathBuf,
}

impl SafetyChecker {
    pub fn new(config: &SafetyConfig) -> Self {
        Self {
            config: config.clone(),
            working_dir: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
        }
    }

    /// Create a safety checker with a specific working directory (test helper)
    #[cfg(test)]
    pub fn with_working_dir(config: &SafetyConfig, working_dir: PathBuf) -> Self {
        Self {
            config: config.clone(),
            working_dir,
        }
    }

    pub fn check_tool_call(&self, call: &ToolCall) -> Result<()> {
        match call.function.name.as_str() {
            "file_write" | "file_edit" | "file_read" => {
                let args: serde_json::Value = serde_json::from_str(&call.function.arguments)?;
                if let Some(path) = args.get("path").and_then(|v| v.as_str()) {
                    self.check_path(path)?;
                }
            }
            "shell_exec" => {
                let args: serde_json::Value = serde_json::from_str(&call.function.arguments)?;
                let cmd = args.get("command").and_then(|v| v.as_str()).unwrap_or("");
                self.check_shell_command(cmd)?;
            }
            "git_commit" | "git_checkpoint" => {
                // Git operations are generally safe
            }
            "git_push" => {
                // Force push should be blocked on protected branches
                let args: serde_json::Value = serde_json::from_str(&call.function.arguments)?;
                if let Some(force) = args.get("force").and_then(|v| v.as_bool()) {
                    if force {
                        anyhow::bail!(
                            "Force push is blocked for safety. Use --no-force or confirm manually."
                        );
                    }
                }
            }
            _ => {}
        }

        Ok(())
    }

    /// Check if a shell command is safe to execute
    fn check_shell_command(&self, cmd: &str) -> Result<()> {
        let dangerous_patterns = [
            "rm -rf /",
            "rm -rf /*",
            "mkfs",
            "dd if=",
            ":(){ :|:& };:", // Fork bomb
            "> /dev/sd",     // Overwrite disk
            "chmod -R 777 /",
            "chown -R",
            "wget -O- | sh", // Pipe to shell
            "curl | sh",
            "curl | bash",
        ];

        for pattern in &dangerous_patterns {
            if cmd.contains(pattern) {
                anyhow::bail!("Dangerous command blocked: {}", pattern);
            }
        }

        // Check for attempts to modify system files via shell
        let system_paths = ["/etc/", "/boot/", "/usr/", "/var/", "/root/"];
        for sys_path in &system_paths {
            if cmd.contains(&format!("rm {}", sys_path))
                || cmd.contains(&format!("rm -rf {}", sys_path))
                || cmd.contains(&format!("> {}", sys_path))
            {
                anyhow::bail!("Command targeting system path blocked: {}", sys_path);
            }
        }

        Ok(())
    }

    /// Canonicalize and check a file path for safety
    fn check_path(&self, path: &str) -> Result<()> {
        // Resolve the path relative to working directory
        let path_buf = Path::new(path);
        let resolved = if path_buf.is_absolute() {
            path_buf.to_path_buf()
        } else {
            self.working_dir.join(path_buf)
        };

        // Attempt to canonicalize (this resolves .. and symlinks)
        // If the file doesn't exist, we normalize manually
        let canonical = resolved.canonicalize().unwrap_or_else(|_| {
            // Manual normalization for non-existent paths
            normalize_path(&resolved)
        });

        let canonical_str = canonical.to_string_lossy();

        // Check for path traversal attempts
        if path.contains("..") {
            // Verify the resolved path is still within expected bounds
            let original_parent = self
                .working_dir
                .canonicalize()
                .unwrap_or_else(|_| self.working_dir.clone());
            if !canonical.starts_with(&original_parent) && !canonical.is_absolute() {
                anyhow::bail!("Path traversal detected: {}", path);
            }
        }

        // Check against denied patterns using the canonical path
        for pattern in &self.config.denied_paths {
            if glob::Pattern::new(pattern)?.matches(&canonical_str) {
                anyhow::bail!("Path matches denied pattern: {}", pattern);
            }
            // Also check the original path for patterns like **/.env
            if glob::Pattern::new(pattern)?.matches(path) {
                anyhow::bail!("Path matches denied pattern: {}", pattern);
            }
        }

        // Check against allowed paths
        if !self.config.allowed_paths.is_empty() {
            let mut allowed = false;
            for pattern in &self.config.allowed_paths {
                // Handle relative patterns by expanding them relative to working directory
                let expanded_pattern = if pattern.starts_with("./") || pattern == "." {
                    // Expand "./**" to "/absolute/path/**"
                    let base = self.working_dir.to_string_lossy();
                    let suffix = pattern.strip_prefix("./").unwrap_or("");
                    format!("{}/{}", base, suffix)
                } else {
                    pattern.clone()
                };

                if glob::Pattern::new(&expanded_pattern)?.matches(&canonical_str)
                    || glob::Pattern::new(pattern)?.matches(&canonical_str)
                    || glob::Pattern::new(pattern)?.matches(path)
                {
                    allowed = true;
                    break;
                }

                // Also check if the canonical path starts with the working directory
                // for the default "./**" pattern
                if pattern == "./**" {
                    let working_dir_str = self.working_dir.to_string_lossy();
                    if canonical_str.starts_with(&*working_dir_str) {
                        allowed = true;
                        break;
                    }
                }
            }
            if !allowed {
                anyhow::bail!("Path not in allowed list: {}", canonical_str);
            }
        }

        Ok(())
    }
}

/// Normalize a path by resolving . and .. components
fn normalize_path(path: &Path) -> PathBuf {
    let mut components = Vec::new();

    for component in path.components() {
        match component {
            std::path::Component::ParentDir => {
                if !components.is_empty() {
                    components.pop();
                }
            }
            std::path::Component::CurDir => {}
            c => components.push(c),
        }
    }

    components.iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::types::{ToolCall, ToolFunction};

    fn create_test_call(name: &str, args: &str) -> ToolCall {
        ToolCall {
            id: "test".to_string(),
            call_type: "function".to_string(),
            function: ToolFunction {
                name: name.to_string(),
                arguments: args.to_string(),
            },
        }
    }

    #[test]
    fn test_safety_checker_new() {
        let config = SafetyConfig::default();
        let checker = SafetyChecker::new(&config);
        assert_eq!(checker.config.allowed_paths, config.allowed_paths);
    }

    #[test]
    fn test_safety_allows_safe_command() {
        let config = SafetyConfig::default();
        let checker = SafetyChecker::new(&config);

        let call = create_test_call("shell_exec", r#"{"command": "ls -la"}"#);
        assert!(checker.check_tool_call(&call).is_ok());
    }

    #[test]
    fn test_safety_blocks_rm_rf_root() {
        let config = SafetyConfig::default();
        let checker = SafetyChecker::new(&config);

        let call = create_test_call("shell_exec", r#"{"command": "rm -rf /"}"#);
        let result = checker.check_tool_call(&call);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Dangerous command blocked"));
    }

    #[test]
    fn test_safety_blocks_mkfs() {
        let config = SafetyConfig::default();
        let checker = SafetyChecker::new(&config);

        let call = create_test_call("shell_exec", r#"{"command": "mkfs.ext4 /dev/sda1"}"#);
        assert!(checker.check_tool_call(&call).is_err());
    }

    #[test]
    fn test_safety_blocks_dd() {
        let config = SafetyConfig::default();
        let checker = SafetyChecker::new(&config);

        let call = create_test_call(
            "shell_exec",
            r#"{"command": "dd if=/dev/zero of=/dev/sda"}"#,
        );
        assert!(checker.check_tool_call(&call).is_err());
    }

    #[test]
    fn test_safety_blocks_fork_bomb() {
        let config = SafetyConfig::default();
        let checker = SafetyChecker::new(&config);

        let call = create_test_call("shell_exec", r#"{"command": ":(){ :|:& };:"}"#);
        assert!(checker.check_tool_call(&call).is_err());
    }

    #[test]
    fn test_safety_allows_file_write_in_allowed_path() {
        let config = SafetyConfig {
            allowed_paths: vec!["./**".to_string()],
            denied_paths: vec![],
            ..Default::default()
        };
        let checker = SafetyChecker::new(&config);

        let call = create_test_call(
            "file_write",
            r#"{"path": "./test.txt", "content": "hello"}"#,
        );
        assert!(checker.check_tool_call(&call).is_ok());
    }

    #[test]
    fn test_safety_blocks_file_write_outside_allowed_path() {
        let config = SafetyConfig {
            allowed_paths: vec!["./safe/**".to_string()],
            denied_paths: vec![],
            ..Default::default()
        };
        let checker = SafetyChecker::new(&config);

        let call = create_test_call(
            "file_write",
            r#"{"path": "/etc/passwd", "content": "hacked"}"#,
        );
        let result = checker.check_tool_call(&call);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Path not in allowed list"));
    }

    #[test]
    fn test_safety_blocks_denied_path() {
        let config = SafetyConfig {
            allowed_paths: vec!["./**".to_string()],
            denied_paths: vec!["**/.env".to_string()],
            ..Default::default()
        };
        let checker = SafetyChecker::new(&config);

        let call = create_test_call(
            "file_write",
            r#"{"path": "./.env", "content": "SECRET=123"}"#,
        );
        let result = checker.check_tool_call(&call);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("denied pattern"));
    }

    #[test]
    fn test_safety_allows_unknown_tool() {
        let config = SafetyConfig::default();
        let checker = SafetyChecker::new(&config);

        let call = create_test_call("unknown_tool", r#"{"arg": "value"}"#);
        assert!(checker.check_tool_call(&call).is_ok());
    }

    #[test]
    fn test_safety_file_edit_uses_same_path_check() {
        let config = SafetyConfig {
            allowed_paths: vec!["./src/**".to_string()],
            denied_paths: vec![],
            ..Default::default()
        };
        let checker = SafetyChecker::new(&config);

        let call = create_test_call(
            "file_edit",
            r#"{"path": "/etc/hosts", "old_str": "a", "new_str": "b"}"#,
        );
        assert!(checker.check_tool_call(&call).is_err());
    }

    #[test]
    fn test_safety_shell_exec_with_missing_command() {
        let config = SafetyConfig::default();
        let checker = SafetyChecker::new(&config);

        // Empty command should be allowed (no dangerous pattern)
        let call = create_test_call("shell_exec", r#"{}"#);
        assert!(checker.check_tool_call(&call).is_ok());
    }

    #[test]
    fn test_check_path_with_multiple_denied_patterns() {
        let config = SafetyConfig {
            allowed_paths: vec!["./**".to_string()],
            denied_paths: vec![
                "**/.env".to_string(),
                "**/secrets/**".to_string(),
                "**/.ssh/**".to_string(),
            ],
            ..Default::default()
        };
        let checker = SafetyChecker::new(&config);

        // Should block .env
        let call1 = create_test_call("file_write", r#"{"path": "./.env", "content": ""}"#);
        assert!(checker.check_tool_call(&call1).is_err());

        // Should block secrets
        let call2 = create_test_call(
            "file_write",
            r#"{"path": "./secrets/api_key.txt", "content": ""}"#,
        );
        assert!(checker.check_tool_call(&call2).is_err());

        // Should block .ssh
        let call3 = create_test_call("file_write", r#"{"path": "./.ssh/id_rsa", "content": ""}"#);
        assert!(checker.check_tool_call(&call3).is_err());
    }

    #[test]
    fn test_check_path_allows_when_no_allowed_paths_configured() {
        let config = SafetyConfig {
            allowed_paths: vec![], // Empty = allow all
            denied_paths: vec![],
            ..Default::default()
        };
        let checker = SafetyChecker::new(&config);

        let call = create_test_call(
            "file_write",
            r#"{"path": "/any/path/at/all.txt", "content": ""}"#,
        );
        assert!(checker.check_tool_call(&call).is_ok());
    }

    // Additional edge case tests for improved coverage

    #[test]
    fn test_safety_with_working_dir() {
        let config = SafetyConfig {
            allowed_paths: vec!["./**".to_string()],
            ..Default::default()
        };
        let checker = SafetyChecker::with_working_dir(&config, PathBuf::from("/home/user/project"));

        // Verify it was constructed with the working dir
        assert!(checker
            .check_tool_call(&create_test_call("file_read", r#"{"path": "./test.txt"}"#))
            .is_ok());
    }

    #[test]
    fn test_safety_blocks_curl_piped_to_sh() {
        let config = SafetyConfig::default();
        let checker = SafetyChecker::new(&config);

        // Pattern is exactly "curl | sh"
        let call = create_test_call("shell_exec", r#"{"command": "curl | sh"}"#);
        assert!(checker.check_tool_call(&call).is_err());
    }

    #[test]
    fn test_safety_blocks_curl_piped_to_bash() {
        let config = SafetyConfig::default();
        let checker = SafetyChecker::new(&config);

        // Pattern is exactly "curl | bash"
        let call = create_test_call("shell_exec", r#"{"command": "curl | bash"}"#);
        assert!(checker.check_tool_call(&call).is_err());
    }

    #[test]
    fn test_safety_blocks_wget_piped_to_sh() {
        let config = SafetyConfig::default();
        let checker = SafetyChecker::new(&config);

        // Pattern is exactly "wget -O- | sh"
        let call = create_test_call("shell_exec", r#"{"command": "wget -O- | sh"}"#);
        assert!(checker.check_tool_call(&call).is_err());
    }

    #[test]
    fn test_safety_blocks_chmod_777_root() {
        let config = SafetyConfig::default();
        let checker = SafetyChecker::new(&config);

        let call = create_test_call("shell_exec", r#"{"command": "chmod -R 777 /"}"#);
        assert!(checker.check_tool_call(&call).is_err());
    }

    #[test]
    fn test_safety_blocks_disk_overwrite() {
        let config = SafetyConfig::default();
        let checker = SafetyChecker::new(&config);

        let call = create_test_call("shell_exec", r#"{"command": "echo data > /dev/sda"}"#);
        assert!(checker.check_tool_call(&call).is_err());
    }

    #[test]
    fn test_safety_blocks_rm_etc() {
        let config = SafetyConfig::default();
        let checker = SafetyChecker::new(&config);

        let call = create_test_call("shell_exec", r#"{"command": "rm /etc/passwd"}"#);
        assert!(checker.check_tool_call(&call).is_err());
    }

    #[test]
    fn test_safety_blocks_rm_rf_etc() {
        let config = SafetyConfig::default();
        let checker = SafetyChecker::new(&config);

        let call = create_test_call("shell_exec", r#"{"command": "rm -rf /etc/"}"#);
        assert!(checker.check_tool_call(&call).is_err());
    }

    #[test]
    fn test_safety_blocks_redirect_to_etc() {
        let config = SafetyConfig::default();
        let checker = SafetyChecker::new(&config);

        let call = create_test_call("shell_exec", r#"{"command": "echo hacked > /etc/passwd"}"#);
        assert!(checker.check_tool_call(&call).is_err());
    }

    #[test]
    fn test_safety_blocks_rm_boot() {
        let config = SafetyConfig::default();
        let checker = SafetyChecker::new(&config);

        let call = create_test_call("shell_exec", r#"{"command": "rm -rf /boot/"}"#);
        assert!(checker.check_tool_call(&call).is_err());
    }

    #[test]
    fn test_safety_blocks_rm_var() {
        let config = SafetyConfig::default();
        let checker = SafetyChecker::new(&config);

        let call = create_test_call("shell_exec", r#"{"command": "rm /var/important"}"#);
        assert!(checker.check_tool_call(&call).is_err());
    }

    #[test]
    fn test_safety_allows_safe_curl() {
        let config = SafetyConfig::default();
        let checker = SafetyChecker::new(&config);

        // curl without piping to shell is OK
        let call = create_test_call("shell_exec", r#"{"command": "curl http://example.com"}"#);
        assert!(checker.check_tool_call(&call).is_ok());
    }

    #[test]
    fn test_safety_allows_safe_wget() {
        let config = SafetyConfig::default();
        let checker = SafetyChecker::new(&config);

        let call = create_test_call(
            "shell_exec",
            r#"{"command": "wget http://example.com/file.tar.gz"}"#,
        );
        assert!(checker.check_tool_call(&call).is_ok());
    }

    #[test]
    fn test_safety_allows_safe_echo() {
        let config = SafetyConfig::default();
        let checker = SafetyChecker::new(&config);

        let call = create_test_call("shell_exec", r#"{"command": "echo hello world"}"#);
        assert!(checker.check_tool_call(&call).is_ok());
    }

    #[test]
    fn test_safety_git_commit_allowed() {
        let config = SafetyConfig::default();
        let checker = SafetyChecker::new(&config);

        let call = create_test_call("git_commit", r#"{"message": "test commit"}"#);
        assert!(checker.check_tool_call(&call).is_ok());
    }

    #[test]
    fn test_safety_git_checkpoint_allowed() {
        let config = SafetyConfig::default();
        let checker = SafetyChecker::new(&config);

        let call = create_test_call("git_checkpoint", r#"{"message": "checkpoint"}"#);
        assert!(checker.check_tool_call(&call).is_ok());
    }

    #[test]
    fn test_safety_git_push_without_force_allowed() {
        let config = SafetyConfig::default();
        let checker = SafetyChecker::new(&config);

        let call = create_test_call("git_push", r#"{"branch": "feature"}"#);
        assert!(checker.check_tool_call(&call).is_ok());
    }

    #[test]
    fn test_safety_git_push_with_force_false_allowed() {
        let config = SafetyConfig::default();
        let checker = SafetyChecker::new(&config);

        let call = create_test_call("git_push", r#"{"branch": "main", "force": false}"#);
        assert!(checker.check_tool_call(&call).is_ok());
    }

    #[test]
    fn test_safety_invalid_json_in_tool_call() {
        let config = SafetyConfig::default();
        let checker = SafetyChecker::new(&config);

        let call = create_test_call("file_read", "not valid json");
        let result = checker.check_tool_call(&call);
        assert!(result.is_err());
    }

    #[test]
    fn test_safety_file_read_uses_path_check() {
        let config = SafetyConfig {
            allowed_paths: vec!["./src/**".to_string()],
            denied_paths: vec![],
            ..Default::default()
        };
        let checker = SafetyChecker::new(&config);

        let call = create_test_call("file_read", r#"{"path": "/etc/passwd"}"#);
        assert!(checker.check_tool_call(&call).is_err());
    }

    #[test]
    fn test_safety_path_traversal_with_double_dots() {
        let config = SafetyConfig {
            allowed_paths: vec!["/home/user/**".to_string()],
            denied_paths: vec![],
            ..Default::default()
        };
        let checker = SafetyChecker::new(&config);

        // Attempting to escape with ..
        let call = create_test_call("file_read", r#"{"path": "/home/user/../../../etc/passwd"}"#);
        let result = checker.check_tool_call(&call);
        // Should be blocked as it resolves outside allowed paths
        assert!(result.is_err());
    }

    #[test]
    fn test_safety_nested_denied_path() {
        let config = SafetyConfig {
            allowed_paths: vec!["./**".to_string()],
            denied_paths: vec!["**/config/secrets/**".to_string()],
            ..Default::default()
        };
        let checker = SafetyChecker::new(&config);

        let call = create_test_call("file_read", r#"{"path": "./config/secrets/api_key.json"}"#);
        assert!(checker.check_tool_call(&call).is_err());
    }

    #[test]
    fn test_normalize_path_handles_parent_dirs() {
        let path = Path::new("/a/b/../c/./d");
        let normalized = normalize_path(path);
        // Should resolve to /a/c/d
        assert!(!normalized.to_string_lossy().contains(".."));
    }

    #[test]
    fn test_normalize_path_handles_current_dir() {
        let path = Path::new("/a/./b/./c");
        let normalized = normalize_path(path);
        // Should not contain .
        let normalized_str = normalized.to_string_lossy();
        assert!(!normalized_str.contains("/./"));
    }

    #[test]
    fn test_normalize_path_empty() {
        let path = Path::new("");
        let normalized = normalize_path(path);
        // Should handle empty path gracefully
        assert!(normalized.to_string_lossy().is_empty() || normalized == PathBuf::new());
    }

    #[test]
    fn test_safety_blocks_rm_slash_star() {
        let config = SafetyConfig::default();
        let checker = SafetyChecker::new(&config);

        let call = create_test_call("shell_exec", r#"{"command": "rm -rf /*"}"#);
        assert!(checker.check_tool_call(&call).is_err());
    }

    #[test]
    fn test_safety_allows_cargo_commands() {
        let config = SafetyConfig::default();
        let checker = SafetyChecker::new(&config);

        let call = create_test_call("shell_exec", r#"{"command": "cargo build --release"}"#);
        assert!(checker.check_tool_call(&call).is_ok());
    }

    #[test]
    fn test_safety_allows_git_commands() {
        let config = SafetyConfig::default();
        let checker = SafetyChecker::new(&config);

        let call = create_test_call("shell_exec", r#"{"command": "git status"}"#);
        assert!(checker.check_tool_call(&call).is_ok());
    }

    #[test]
    fn test_safety_blocks_chown_recursive() {
        let config = SafetyConfig::default();
        let checker = SafetyChecker::new(&config);

        let call = create_test_call("shell_exec", r#"{"command": "chown -R root:root /"}"#);
        assert!(checker.check_tool_call(&call).is_err());
    }
}
