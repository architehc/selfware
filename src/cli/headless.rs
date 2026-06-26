//! Structured headless output protocol for machine-readable agent results.
//!
//! When `--output-format json` or `--output-format stream-json` is used, the
//! CLI emits JSON to stdout instead of human-readable text.  Diagnostics and
//! logs continue to go to stderr so stdout stays pure JSON.

use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;

use crate::agent::progress::{ProgressEmitter, ProgressEvent};
use crate::observability::dashboard::TokenUsage;

/// Final session result emitted once at the end of a headless run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionResult {
    pub session_id: String,
    pub exit_status: i32,
    pub stop_reason: String,
    pub num_turns: usize,
    pub patch_bytes: usize,
    pub patch_lines: usize,
    pub usage: TokenUsage,
    pub model: String,
    pub duration_ms: u64,
    pub failure_mode: Option<String>,
    pub artifact_dir: Option<PathBuf>,
}

/// Individual event emitted in `--output-format stream-json` mode.
#[derive(Debug, Clone, Serialize)]
pub struct HeadlessEvent {
    pub event: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub step: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub args: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ok: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub outcome: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

impl HeadlessEvent {
    pub fn step_started(step: usize, model: String) -> Self {
        Self {
            event: "step_started",
            step: Some(step),
            model: Some(model),
            tool: None,
            args: None,
            ok: None,
            outcome: None,
            reason: None,
        }
    }

    pub fn tool_call_started(tool: String, args: String) -> Self {
        Self {
            event: "tool_call_started",
            step: None,
            model: None,
            tool: Some(tool),
            args: Some(args),
            ok: None,
            outcome: None,
            reason: None,
        }
    }

    pub fn tool_call_completed(tool: String, ok: bool) -> Self {
        Self {
            event: "tool_call_completed",
            step: None,
            model: None,
            tool: Some(tool),
            args: None,
            ok: Some(ok),
            outcome: None,
            reason: None,
        }
    }

    pub fn step_completed(step: usize) -> Self {
        Self {
            event: "step_completed",
            step: Some(step),
            model: None,
            tool: None,
            args: None,
            ok: None,
            outcome: None,
            reason: None,
        }
    }

    pub fn task_completed(outcome: String) -> Self {
        Self {
            event: "task_completed",
            step: None,
            model: None,
            tool: None,
            args: None,
            ok: None,
            outcome: Some(outcome),
            reason: None,
        }
    }

    pub fn task_failed(reason: String) -> Self {
        Self {
            event: "task_failed",
            step: None,
            model: None,
            tool: None,
            args: None,
            ok: None,
            outcome: None,
            reason: Some(reason),
        }
    }
}

/// Emit a single headless event as JSON to stdout.
#[allow(dead_code)]
pub fn emit_event(event: &HeadlessEvent) {
    if let Ok(json) = serde_json::to_string(event) {
        use std::io::Write;
        let stdout = std::io::stdout();
        let mut lock = stdout.lock();
        let _ = writeln!(lock, "{}", json);
    }
}

/// Emit the final session result as JSON to stdout.
pub fn emit_result(result: &SessionResult) {
    if let Ok(json) = serde_json::to_string(result) {
        use std::io::Write;
        let stdout = std::io::stdout();
        let mut lock = stdout.lock();
        let _ = writeln!(lock, "{}", json);
    }
}

/// Capture `git diff` from the current working directory, including newly
/// added files and excluding selfware-internal scratch directories.
pub fn capture_patch() -> anyhow::Result<String> {
    let _ = std::process::Command::new("git")
        .args(["add", "-A"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();

    let out = std::process::Command::new("git")
        .args([
            "diff",
            "--cached",
            "HEAD",
            "--",
            ".",
            ":(exclude).selfware/**",
            ":(exclude).claude/**",
            ":(exclude)__pycache__/**",
            ":(exclude)**/__pycache__/**",
            ":(exclude)selfware.toml",
            ":(exclude)*.bak",
            ":(exclude)**/*.bak",
        ])
        .output()
        .map_err(|e| anyhow::anyhow!("running git diff: {}", e))?;

    if !out.status.success() {
        anyhow::bail!(
            "git diff failed (status={:?}): {}",
            out.status.code(),
            String::from_utf8_lossy(&out.stderr)
        );
    }
    String::from_utf8(out.stdout).map_err(|e| anyhow::anyhow!("non-UTF8 patch: {}", e))
}

/// Progress emitter that writes newline-delimited JSON to stdout.
pub struct JsonlProgressEmitter {
    lock: Mutex<()>,
}

impl JsonlProgressEmitter {
    pub fn new() -> Self {
        Self {
            lock: Mutex::new(()),
        }
    }
}

impl Default for JsonlProgressEmitter {
    fn default() -> Self {
        Self::new()
    }
}

impl ProgressEmitter for JsonlProgressEmitter {
    fn emit(&self, event: ProgressEvent) {
        let headless = match event {
            ProgressEvent::StepStarted { step, model, .. } => {
                Some(HeadlessEvent::step_started(step, model))
            }
            ProgressEvent::ToolCallStarted { tool, args_short } => {
                Some(HeadlessEvent::tool_call_started(tool, args_short))
            }
            ProgressEvent::ToolCallCompleted { tool, ok, .. } => {
                Some(HeadlessEvent::tool_call_completed(tool, ok))
            }
            ProgressEvent::StepCompleted { step, .. } => Some(HeadlessEvent::step_completed(step)),
            ProgressEvent::TaskCompleted { outcome } => {
                Some(HeadlessEvent::task_completed(outcome))
            }
            ProgressEvent::TaskFailed { reason } => Some(HeadlessEvent::task_failed(reason)),
            _ => None,
        };

        if let Some(ev) = headless {
            let _guard = self.lock.lock().unwrap_or_else(|e| e.into_inner());
            let mut stdout = std::io::stdout().lock();
            if let Ok(json) = serde_json::to_string(&ev) {
                let _ = writeln!(stdout, "{}", json);
                let _ = stdout.flush();
            }
        }
    }
}
