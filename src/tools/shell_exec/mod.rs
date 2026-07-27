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

/// A `sed -i 's/old/new/[g]' file` invocation simple enough to intercept.
struct SedSubstitution {
    file: String,
    old_str: String,
    new_str: String,
    global: bool,
    /// `-i.bak` / `--in-place=.bak` backup suffix, if any.
    backup_suffix: Option<String>,
}

/// Attempt to parse a straightforward `sed -i 's/old/new/g' file` substitution.
/// Returns the parsed substitution if it is in the safe literal subset we can
/// faithfully reproduce, or `None` for anything that must run through the real
/// sed binary: empty patterns, regex metacharacters in the pattern, `&`/`\`
/// escapes in the replacement, flags other than ``/`g`, multiple files, extra
/// sed flags, or unquoted scripts.
fn try_parse_sed_substitution(command: &str) -> Option<SedSubstitution> {
    let trimmed = command.trim();

    // First token must be exactly `sed`.
    let after_sed = trimmed.strip_prefix("sed")?;
    if !after_sed.is_empty() && !after_sed.starts_with(char::is_whitespace) {
        return None;
    }

    // Find the quoted substitution expression: look for 's... or "s...
    let (quote_idx, quote_char) = trimmed
        .char_indices()
        .find(|(_, c)| *c == '\'' || *c == '"')?;

    // Between `sed` and the opening quote only in-place flags may appear;
    // anything else (addresses, -e, -n, -r, unquoted script, ...) → real sed.
    let mut saw_in_place = false;
    let mut backup_suffix: Option<String> = None;
    for token in trimmed["sed".len()..quote_idx].split_whitespace() {
        if token == "-i" || token == "--in-place" {
            saw_in_place = true;
        } else if token.starts_with("-i") && token.len() > 2 {
            // GNU attached-suffix form: -i.bak
            saw_in_place = true;
            backup_suffix = Some(token[2..].to_string());
        } else {
            let suffix = token.strip_prefix("--in-place=")?;
            saw_in_place = true;
            backup_suffix = Some(suffix.to_string());
        }
    }
    if !saw_in_place {
        return None;
    }
    // A backup suffix using shell globbing is beyond the safe subset.
    if backup_suffix
        .as_deref()
        .is_some_and(|s| s.contains('*') || s.is_empty())
    {
        return None;
    }

    let after_quote = &trimmed[quote_idx + 1..];
    let sub_end = after_quote.find(quote_char)?;
    let sub_expr = &after_quote[..sub_end]; // e.g. "s/old/new/g"

    if !sub_expr.starts_with('s') {
        return None; // y///, d, etc. → real sed
    }
    let delimiter = sub_expr.chars().nth(1)?;
    // sed forbids alphanumeric, backslash and whitespace delimiters.
    if delimiter.is_alphanumeric() || delimiter == '\\' || delimiter.is_whitespace() {
        return None;
    }
    let rest = &sub_expr[1 + delimiter.len_utf8()..];
    let mut parts = rest.split(delimiter);
    let old_str = parts.next()?;
    let new_str = parts.next()?;
    // Unterminated `s` command (no flags segment) — real sed errors on it;
    // don't improvise.
    let flags = parts.next()?;
    if parts.next().is_some() {
        return None; // extra unescaped delimiters → real sed's error
    }
    if !flags.is_empty() && flags != "g" {
        return None; // numeric / p / w / i flags → real sed
    }
    let global = flags == "g";

    // Empty pattern: sed reuses the previous regex (none exists here, so real
    // sed errors). Outside the safe subset — let real sed report it.
    if old_str.is_empty() {
        return None;
    }
    // Only intercept literal patterns (no regex metacharacters).
    if old_str.contains([
        '.', '*', '+', '?', '^', '$', '[', ']', '(', ')', '{', '}', '|', '\\',
    ]) {
        return None;
    }
    // Only intercept literal replacements (no `&` expansion or `\` escapes).
    if new_str.contains(['&', '\\']) {
        return None;
    }

    // Extract filename: text after the closing quote, trimmed
    let after_sub = after_quote[sub_end + 1..].trim();
    if after_sub.is_empty() {
        return None;
    }
    let file_tokens: Vec<&str> = after_sub.split_whitespace().collect();
    if file_tokens.len() != 1 {
        return None; // Multiple files or extra args — too complex
    }
    let file = file_tokens[0];
    if file.starts_with('-') {
        return None;
    }

    Some(SedSubstitution {
        file: file.to_string(),
        old_str: old_str.to_string(),
        new_str: new_str.to_string(),
        global,
        backup_suffix,
    })
}

/// Resolve the sed target path the way the real command would: a relative path
/// is interpreted against the call's `cwd`, not the agent's process cwd.
fn resolve_sed_path(file: &str, cwd: Option<&str>) -> String {
    if Path::new(file).is_absolute() {
        return file.to_string();
    }
    match cwd {
        Some(dir) => Path::new(dir).join(file).to_string_lossy().into_owned(),
        None => file.to_string(),
    }
}

/// Replace the first occurrence of `old` on EACH line — real sed's non-`g`
/// `s///` semantics operate per line, not on the whole file at once.
/// Returns the new content and the number of lines changed.
fn replace_first_per_line(content: &str, old: &str, new: &str) -> (String, usize) {
    let mut changed = 0usize;
    let out = content
        .split_inclusive('\n')
        .map(|segment| {
            let (body, newline) = match segment.strip_suffix('\n') {
                Some(body) => (body, "\n"),
                None => (segment, ""),
            };
            if body.contains(old) {
                changed += 1;
                format!("{}{}", body.replacen(old, new, 1), newline)
            } else {
                segment.to_string()
            }
        })
        .collect();
    (out, changed)
}

/// Apply a parsed sed substitution with stale-guard protection, mirroring real
/// GNU sed semantics for the intercepted literal subset: without `g` the first
/// match ON EACH LINE is replaced, `-i<suffix>` keeps a backup of the original
/// at `<file><suffix>`, and a no-match substitution exits 0 without touching
/// the file.
async fn apply_sed_substitution(file: &str, sub: &SedSubstitution) -> anyhow::Result<Value> {
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

    let (new_content, replacements) = if sub.global {
        let count = content.matches(&sub.old_str).count();
        (content.replace(&sub.old_str, &sub.new_str), count)
    } else {
        replace_first_per_line(&content, &sub.old_str, &sub.new_str)
    };

    if replacements > 0 {
        // Honor `-i<suffix>`: keep a backup of the original, like real sed.
        if let Some(suffix) = &sub.backup_suffix {
            tokio::fs::copy(file, format!("{}{}", file, suffix)).await?;
        }
        write_atomic(Path::new(file), &new_content).await?;
    }
    // A no-match substitution is NOT an error: real sed exits 0 and leaves the
    // file untouched. Report the zero replacements honestly instead of either
    // corrupting the file or inventing a failure.

    Ok(serde_json::json!({
        "exit_code": 0,
        "stdout": "",
        "stderr": "",
        "stdout_pagination": {"offset":0,"limit":10000,"total_chars":0,"has_more":false},
        "stderr_pagination": {"offset":0,"limit":10000,"total_chars":0,"has_more":false},
        "duration_ms": 0,
        "timed_out": false,
        "intercepted": true,
        "replacements": replacements,
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
            // The structured env map must not smuggle what the command-string
            // DANGEROUS_ENV_VARS check would block (loader injection,
            // interpreter startup hooks, PATH hijack). Both enforce the same
            // shared list — see DENIED_ENV_VARS for what's deliberately NOT
            // denied (ENV, LD_DEBUG, HOME, …).
            let upper = name.to_ascii_uppercase();
            if crate::safety::checker::validation::DENIED_ENV_VARS.contains(&upper.as_str()) {
                anyhow::bail!(
                    "Environment variable '{}' is not allowed in shell_exec env (injection risk)",
                    name
                );
            }
        }

        // Intercept simple sed -i substitutions and route through file tools
        // for stale-guard protection. Only the safe literal subset parses;
        // anything more complex (real regexes, empty patterns, unusual flags)
        // falls through to the real sed binary with real sed semantics.
        if let Some(sub) = try_parse_sed_substitution(&args.command) {
            // Real sed resolves relative paths against the command's cwd; the
            // interception must do the same or it would edit the wrong file.
            let resolved = resolve_sed_path(&sub.file, args.cwd.as_deref());
            // Intercept only when the resolved path passes validation;
            // otherwise let the real sed run under the shell like any command.
            let safety = crate::tools::file::resolve_safety_config(None);
            if crate::tools::file::validate_tool_path(&resolved, &safety).is_ok() {
                return apply_sed_substitution(&resolved, &sub).await;
            }
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
#[path = "../../../tests/unit/tools/shell_exec/mod_test.rs"]
mod tests;
