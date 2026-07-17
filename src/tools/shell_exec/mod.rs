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

/// Heuristic: does `command` invoke a build/test/compile that routinely exceeds
/// the 60s shell default on large projects? Used to raise the timeout floor so
/// the command can finish and report instead of being killed with empty output.
fn command_is_long_running_build(command: &str) -> bool {
    let c = command.to_lowercase();
    const MARKERS: &[&str] = &[
        "cargo build",
        "cargo check",
        "cargo test",
        "cargo clippy",
        "cargo bench",
        "cargo install",
        "cargo doc",
        "cargo run",
        "cmake",
        "ninja",
        "npm run build",
        "npm install",
        "npm ci",
        "yarn build",
        "pnpm build",
        "go build",
        "go test",
        "gradle",
        "bazel",
        "pip install",
    ];
    MARKERS.iter().any(|m| c.contains(m))
}

/// Maximum bytes captured per stream from a shell command. Output beyond this
/// is still drained (so the child never blocks on a full pipe) but discarded,
/// so a runaway process can't OOM the agent before the timeout reaps it.
const MAX_CAPTURE_BYTES: usize = 10 * 1024 * 1024; // 10 MiB

/// Read a child pipe to EOF, keeping at most `MAX_CAPTURE_BYTES` in memory while
/// still consuming the rest so the process isn't blocked on a full pipe.
async fn drain_capped<R: tokio::io::AsyncRead + Unpin>(mut reader: R) -> Vec<u8> {
    use tokio::io::AsyncReadExt;
    let mut buf = Vec::new();
    let mut chunk = [0u8; 8192];
    loop {
        match reader.read(&mut chunk).await {
            Ok(0) | Err(_) => break,
            Ok(n) => {
                if buf.len() < MAX_CAPTURE_BYTES {
                    let take = n.min(MAX_CAPTURE_BYTES - buf.len());
                    buf.extend_from_slice(&chunk[..take]);
                }
                // Beyond the cap: keep reading to drain the pipe, discard excess.
            }
        }
    }
    buf
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

        // Build/test commands routinely take far longer than the 60s default on
        // large projects. Killing them at 60s returns empty output, and the model
        // — unable to see the result — loops re-running the command (observed:
        // `cargo check` self-improvement runs spinning to MAX_ITERATIONS). Give
        // known long-running build/test commands a generous floor so they can
        // actually finish and report.
        if command_is_long_running_build(&args.command) {
            args.timeout_secs = args.timeout_secs.max(600);
        }

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

        // Run the child in its own process group so a timeout can reap the
        // ENTIRE process tree (grandchildren included, e.g. cargo -> rustc), not
        // just the direct shell — `kill_on_drop`/`child.kill()` only signal the
        // immediate child, so a backgrounded subprocess would otherwise orphan
        // into a defunct zombie holding target/ locks.
        #[cfg(unix)]
        cmd.process_group(0);
        cmd.stdin(std::process::Stdio::null());
        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::piped());

        let start = std::time::Instant::now();
        let mut child = cmd.spawn()?;
        let child_pid = child.id();

        // Drain stdout/stderr concurrently (bounded) so a chatty process can't
        // deadlock on a full pipe or OOM the agent with unbounded output.
        let stdout_pipe = child.stdout.take();
        let stderr_pipe = child.stderr.take();
        let stdout_task = tokio::spawn(async move {
            match stdout_pipe {
                Some(s) => drain_capped(s).await,
                None => Vec::new(),
            }
        });
        let stderr_task = tokio::spawn(async move {
            match stderr_pipe {
                Some(s) => drain_capped(s).await,
                None => Vec::new(),
            }
        });

        let wait_result =
            tokio::time::timeout(Duration::from_secs(args.timeout_secs), child.wait()).await;

        let (exit_code, timed_out) = match wait_result {
            Ok(Ok(status)) => (status.code().unwrap_or(-1), false),
            Ok(Err(e)) => return Err(e.into()),
            Err(_) => {
                // Timed out: kill the whole process group, then reap the child.
                #[cfg(unix)]
                if let Some(pid) = child_pid {
                    use nix::sys::signal::{killpg, Signal};
                    use nix::unistd::Pid;
                    let _ = killpg(Pid::from_raw(pid as i32), Signal::SIGKILL);
                }
                let _ = child.kill().await;
                let _ = child.wait().await;
                (-1, true)
            }
        };

        // Always await the drain tasks so they don't leak; the pipes close once
        // the process (and its group) exit.
        let stdout_bytes = stdout_task.await.unwrap_or_default();
        let stderr_bytes = stderr_task.await.unwrap_or_default();
        let stdout = String::from_utf8_lossy(&stdout_bytes).into_owned();
        let stderr = if timed_out {
            format!(
                "Command timed out after {}s and was killed (whole process group \
                 reaped). For long-running builds/tests (e.g. `cargo \
                 check`/`build`/`test`), retry the SAME command with a larger \
                 timeout, e.g. add \"timeout_secs\": 600 to the tool arguments.",
                args.timeout_secs
            )
        } else {
            String::from_utf8_lossy(&stderr_bytes).into_owned()
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
    #[cfg(unix)]
    async fn registered_shell_exec_timeout_reaps_process_group() {
        // The REGISTERED shell tool must reap the whole tree on timeout — a
        // backgrounded grandchild (`sleep 30 &`) must be killed, not orphaned.
        let dir = tempfile::tempdir().unwrap();
        let pidfile = dir.path().join("gc.pid");
        let tool = ShellExec;
        let args = serde_json::json!({
            "command": format!("sleep 30 & echo $! > {}; wait", pidfile.display()),
            "timeout_secs": 1
        });
        let start = std::time::Instant::now();
        let result = tool.execute(args).await.unwrap();
        assert!(start.elapsed().as_secs() < 10, "should return at timeout");
        assert_eq!(result["timed_out"], true);

        let gc_pid: i32 = std::fs::read_to_string(&pidfile)
            .expect("grandchild wrote its pid")
            .trim()
            .parse()
            .expect("valid pid");
        tokio::time::sleep(Duration::from_millis(500)).await;
        #[cfg(unix)]
        {
            use nix::sys::signal::kill;
            use nix::unistd::Pid;
            let alive = kill(Pid::from_raw(gc_pid), None).is_ok();
            assert!(!alive, "grandchild pid {gc_pid} should have been reaped");
        }
    }

    #[tokio::test]
    async fn registered_shell_exec_large_output_completes() {
        // ~40 MiB of output must complete without hang/OOM (bounded drain).
        let tool = ShellExec;
        let args = serde_json::json!({
            "command": "head -c 41943040 /dev/zero | tr '\\0' 'a'",
            "timeout_secs": 30
        });
        let result = tool.execute(args).await.unwrap();
        assert_eq!(result["timed_out"], false);
        assert_eq!(result["exit_code"], 0);
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
    #[cfg(unix)]
    async fn test_shell_exec_timeout() {
        let tool = ShellExec;
        let args = serde_json::json!({
            "command": "sleep 10",
            "timeout_secs": 1
        });

        let result = tool.execute(args).await.unwrap();
        assert_eq!(result["timed_out"], true);
        let stderr = result["stderr"].as_str().unwrap();
        assert!(stderr.contains("timed out"));
        // Regression: the timeout must be actionable so the model retries with a
        // larger timeout instead of looping (e.g. `cargo check` on big crates).
        assert!(stderr.contains("timeout_secs"), "stderr was: {stderr}");
    }

    #[test]
    fn test_command_is_long_running_build() {
        // Build/test commands get a generous timeout floor so they aren't killed
        // at 60s with empty output (which made the model loop on `cargo check`).
        assert!(command_is_long_running_build("cargo check --all-targets"));
        assert!(command_is_long_running_build(
            "cd foo && cargo build --release"
        ));
        assert!(command_is_long_running_build("npm install"));
        assert!(!command_is_long_running_build("ls -la"));
        assert!(!command_is_long_running_build("grep -r foo src/"));
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
        // Keep the temp directory under the current working dir so the default
        // safety config (`./**`) allows the intercepted path validation.
        let temp_dir = std::env::current_dir()
            .unwrap()
            .join(format!("selfware-sed-test-{}", std::process::id()));
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
