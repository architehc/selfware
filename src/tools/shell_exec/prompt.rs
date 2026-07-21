//! Rich LLM-facing descriptions for the shell_exec tool.

use crate::tools::prompt::{ToolExample, ToolPrompt};

/// Rich prompt implementation for ShellExec tool.
pub struct ShellExecPrompt;

impl Default for ShellExecPrompt {
    fn default() -> Self {
        Self::new()
    }
}

impl ShellExecPrompt {
    /// Create a new ShellExecPrompt instance.
    pub fn new() -> Self {
        Self
    }
}

impl ToolPrompt for ShellExecPrompt {
    fn description(&self) -> String {
        "Execute shell commands. Use this for builds, tests, package installations, \
         git operations, and system administration tasks. \\n\\n\
         Commands run with a configurable timeout (default 60s, max 1 hour) and \
         return exit code, stdout, stderr, and execution duration."
            .to_string()
    }

    fn when_to_use(&self) -> String {
        "Use shell_exec when:\\n\
         - Running build commands (cargo build, npm run build, make, etc.)\\n\
         - Executing tests (cargo test, npm test, pytest, etc.)\\n\
         - Installing packages or dependencies\\n\
         - Running git commands (status, log, branch, etc.)\\n\
         - Checking system information (disk usage, running processes)\\n\
         - Any task that requires command-line tools\\n\\n\
         **Prefer other tools when:**\\n\
         - Reading files: Use file_read instead (handles encodings better)\\n\
         - Writing files: Use file_write instead (safer, atomic writes)\\n\
         - Editing files: Use file_edit instead (surgical string replacement)\\n\
         - Searching code: Use grep_search instead (with context and pagination)\\n\
         - Finding files: Use glob_find instead (structured results)"
            .to_string()
    }

    fn examples(&self) -> Vec<ToolExample> {
        vec![
            ToolExample {
                description: "Run a simple command with output".to_string(),
                input: serde_json::json!({
                    "command": "echo 'Hello, World!'",
                    "timeout_secs": 30
                }),
                output: Some(
                    "Returns exit_code 0, stdout containing 'Hello, World!', empty stderr, \
                     and duration_ms. Use this pattern for quick checks and simple commands."
                        .to_string(),
                ),
            },
            ToolExample {
                description: "Build a Rust project and capture output".to_string(),
                input: serde_json::json!({
                    "command": "cargo build --release",
                    "timeout_secs": 300
                }),
                output: Some(
                    "Returns build output in stdout/stderr and exit_code. \
                     Check exit_code to determine if build succeeded (0) or failed (non-zero)."
                        .to_string(),
                ),
            },
            ToolExample {
                description: "Run tests with custom working directory and environment".to_string(),
                input: serde_json::json!({
                    "command": "cargo test",
                    "cwd": "/home/user/myproject",
                    "timeout_secs": 120,
                    "env": {
                        "RUST_BACKTRACE": "1",
                        "TEST_THREADS": "4"
                    }
                }),
                output: Some(
                    "Runs tests in the specified directory with custom environment variables. \
                     Use env to set variables like RUST_BACKTRACE for debugging."
                        .to_string(),
                ),
            },
            ToolExample {
                description: "Check disk usage of the current directory".to_string(),
                input: serde_json::json!({
                    "command": "du -sh .",
                    "timeout_secs": 60
                }),
                output: Some(
                    "Returns the total size of the current directory. \
                     Useful for understanding project size or available space."
                        .to_string(),
                ),
            },
        ]
    }

    fn important_notes(&self) -> Option<String> {
        Some(
            "**Important Notes:**\\n\
             - Default timeout: 60 seconds (configurable up to 1 hour)\\n\
             - Commands are executed in a shell (/bin/sh on Unix, cmd on Windows)\\n\
             - Working directory (cwd) must be an absolute path without '..' components\\n\
             - Environment variable names cannot contain '=' or null bytes\\n\
             - Maximum command length: 10,000 characters\\n\
             - Dangerous patterns are blocked (e.g., /dev/tcp/, mkfifo, pipes to interactive shells)\\n\
             - Output is paginated if it exceeds the limit (use output_offset to get more)\\n\
             - Commands that timeout return exit_code -1 and timed_out: true"
                .to_string(),
        )
    }
}

/// Get the rich description for shell_exec tool.
pub fn get_description() -> String {
    ShellExecPrompt::new().description()
}

/// Get the rich when_to_use guidance for shell_exec tool.
pub fn get_when_to_use() -> String {
    ShellExecPrompt::new().when_to_use()
}

/// Get examples for shell_exec tool.
pub fn get_examples() -> Vec<ToolExample> {
    ShellExecPrompt::new().examples()
}

/// Get important notes for shell_exec tool.
pub fn get_important_notes() -> Option<String> {
    ShellExecPrompt::new().important_notes()
}

#[cfg(test)]
#[path = "../../../tests/unit/tools/shell_exec/prompt/prompt_test.rs"]
mod tests;
