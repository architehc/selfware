//! Structured headless output protocol for machine-readable agent results.
//!
//! When `--output-format json` or `--output-format stream-json` is used, the
//! CLI emits JSON to stdout instead of human-readable text.  Diagnostics and
//! logs continue to go to stderr so stdout stays pure JSON.

use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use crate::agent::progress::{ProgressEmitter, ProgressEvent};
use crate::agent::tui_events::{AgentEvent, EventEmitter};
use crate::config::ExecutionMode;
use crate::observability::dashboard::TokenUsage;
use crate::safety::process_env::SanitizedEnvExt;

/// Reason a headless run must not start, or `None` when it may proceed.
///
/// In `Normal` execution mode every mutating tool requires an interactive
/// confirmation. When stdin is not a terminal (piped, cron, CI, `selfware -p`
/// from a script) there is nobody to answer, so every write tool is refused,
/// the model flails for the whole turn budget burning real API tokens, and the
/// run finally dies at MAX_ITERATIONS with advice pointing the wrong way.
/// Detecting this up front lets the CLI fail fast BEFORE any LLM call with a
/// message that names the real cause and the fix.
pub fn headless_mode_block_reason(mode: ExecutionMode, stdin_is_terminal: bool) -> Option<String> {
    if mode == ExecutionMode::Normal && !stdin_is_terminal {
        Some(
            "headless run in `--mode normal` cannot confirm tool executions \
             (stdin is not a terminal): every write tool would be refused and the \
             agent would spend tokens until MAX_ITERATIONS without making progress. \
             Re-run with `-m yolo` or `-m auto-edit`, e.g. `selfware -m yolo -p \"...\"`."
                .to_string(),
        )
    } else {
        None
    }
}

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
    /// What the agent concluded: the final assistant response, trimmed.
    ///
    /// `None` when the run ended without a text answer (never started, failed
    /// before answering, or completed with an empty terminal message). Omitted
    /// from the JSON when `None`, so pre-existing consumers keep seeing a
    /// byte-identical shape for runs without an answer.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub answer: Option<String>,
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

    pub fn turn_decision(decision: String, detail: String) -> Self {
        // Reuse the `reason` field for the decision name and `outcome` for the
        // detail so the JSON shape stays backward-compatible with existing
        // consumers (no new struct fields needed).
        Self {
            event: "turn_decision",
            step: None,
            model: None,
            tool: None,
            args: None,
            ok: None,
            outcome: if detail.is_empty() {
                None
            } else {
                Some(detail)
            },
            reason: Some(decision),
        }
    }
}

/// Emit a single headless event as JSON to stdout.
#[allow(dead_code)]
pub fn emit_event(event: &HeadlessEvent) {
    if let Some(json) = serde_json::to_string(event)
        .ok()
        .and_then(validated_jsonl_line)
    {
        use std::io::Write;
        let stdout = std::io::stdout();
        let mut lock = stdout.lock();
        let _ = writeln!(lock, "{}", json);
    }
}

/// Emit the final session result as JSON to stdout.
pub fn emit_result(result: &SessionResult) {
    if let Some(json) = serde_json::to_string(result)
        .ok()
        .and_then(validated_jsonl_line)
    {
        use std::io::Write;
        let stdout = std::io::stdout();
        let mut lock = stdout.lock();
        let _ = writeln!(lock, "{}", json);
    }
}

/// Captures the agent's final answer for the structured [`SessionResult`].
///
/// The answer text lives in the agent's private `last_assistant_response`
/// field, which the CLI cannot read directly. The one public conduit is the
/// [`EventEmitter`] channel attached with `Agent::with_event_emitter`: the
/// run-end wrapper emits `AgentEvent::Completed { message }` whose `message`
/// IS `last_assistant_response.trim()` (see `run_execution_loop` in
/// `task_runner.rs`). Attach [`AnswerCapture::emitter`] to the agent before
/// the run and read [`AnswerCapture::take`] afterwards — the headless runner
/// then serializes exactly what the agent concluded as `SessionResult.answer`.
pub struct AnswerCapture {
    last: Arc<Mutex<Option<String>>>,
}

impl AnswerCapture {
    pub fn new() -> Self {
        Self {
            last: Arc::new(Mutex::new(None)),
        }
    }

    /// An emitter to wire into the agent via `Agent::with_event_emitter`.
    pub fn emitter(&self) -> AnswerCaptureEmitter {
        AnswerCaptureEmitter {
            last: Arc::clone(&self.last),
        }
    }

    /// Take the captured final answer (one-shot; `None` when the run never
    /// completed with a non-empty response, e.g. a failed run).
    pub fn take(&self) -> Option<String> {
        self.last.lock().ok().and_then(|mut guard| guard.take())
    }
}

impl Default for AnswerCapture {
    fn default() -> Self {
        Self::new()
    }
}

/// The [`EventEmitter`] half of [`AnswerCapture`] — see its docs.
pub struct AnswerCaptureEmitter {
    last: Arc<Mutex<Option<String>>>,
}

impl EventEmitter for AnswerCaptureEmitter {
    fn emit(&self, event: AgentEvent) {
        // Only the terminal `Completed` event carries the answer; `Error` is
        // the run's diagnostic, not what the agent concluded, so it is not
        // captured as an answer.
        if let AgentEvent::Completed { message } = event {
            let trimmed = message.trim().to_string();
            if !trimmed.is_empty() {
                if let Ok(mut guard) = self.last.lock() {
                    *guard = Some(trimmed);
                }
            }
        }
    }
}

/// Capture `git diff` from the current working directory, including newly
/// added files and excluding selfware-internal scratch directories.
/// Whether the repository in the current directory has a resolvable `HEAD`
/// (i.e. at least one commit). Used to tell a legitimately-empty repo (no HEAD)
/// from a genuine seeding failure.
fn head_exists() -> bool {
    std::process::Command::new("git")
        .sanitized_env()
        .args(["rev-parse", "--verify", "HEAD"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

pub fn capture_patch() -> anyhow::Result<String> {
    // Stage changes into a *temporary* git index so we never mutate the user's
    // real `.git/index`. `GIT_INDEX_FILE` redirects `git add`/`git diff` at a
    // throwaway index. We use a temp *directory* (not a pre-created temp file)
    // because git rejects a zero-byte existing index file; the index file path
    // inside the temp dir does not pre-exist, so git creates a fresh index.
    // The TempDir auto-deletes the index on drop, keeping the user's real index
    // (`git status` / `.git/index`) exactly as the user had it.
    let tmp_index_dir = tempfile::Builder::new()
        .prefix("selfware-index-")
        .tempdir()
        .map_err(|e| anyhow::anyhow!("creating temp dir for git index: {}", e))?;
    let tmp_index = tmp_index_dir.path().join("index");

    // Seed the temporary index from HEAD before staging. Without this the index
    // starts EMPTY, so `git add -A` — which skips paths matched by `.gitignore`
    // — never re-adds tracked-but-ignored files, and `git diff --cached HEAD`
    // then reports every one of them as a spurious deletion (the giant phantom
    // patch: files that exist and are committed, reported as removed). Seeding
    // from HEAD makes those files present in the temp index, so an unchanged
    // tracked-ignored file stays unchanged and only real working-tree edits
    // (modifications, new files, genuine deletions) appear in the patch.
    let read_tree = std::process::Command::new("git")
        .sanitized_env()
        .env("GIT_INDEX_FILE", &tmp_index)
        .args(["read-tree", "HEAD"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();
    // If seeding failed while a HEAD actually exists, the index would start
    // empty and every tracked-but-.gitignore'd file would show as a phantom
    // deletion — refuse rather than emit a bogus patch. A repo with no commits
    // legitimately has no HEAD; there the empty index is correct and the
    // diff-vs-HEAD below fails cleanly on its own.
    let seed_ok = matches!(read_tree, Ok(ref s) if s.success());
    if !seed_ok && head_exists() {
        anyhow::bail!("git read-tree HEAD failed while seeding the patch index");
    }

    let add_status = std::process::Command::new("git")
        .sanitized_env()
        .env("GIT_INDEX_FILE", &tmp_index)
        .args(["add", "-A"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();
    if !matches!(add_status, Ok(ref s) if s.success()) {
        anyhow::bail!("git add -A failed while staging the patch");
    }

    // A freshly `git init`-ed repo has no HEAD: `git diff --cached HEAD`
    // fails there and a fully-successful headless task would be reported as
    // patch_capture_failed. Diff against the standard empty-tree object
    // instead, so a brand-new repo reports its staged files as additions (or
    // an empty patch when nothing exists) rather than erroring. A genuine
    // diff failure with a real HEAD still bails below.
    let diff_base = if head_exists() {
        "HEAD"
    } else {
        "4b825dc642cb6eb9a060e54bf8d69288fbee4904"
    };
    let out = std::process::Command::new("git")
        .sanitized_env()
        .env("GIT_INDEX_FILE", &tmp_index)
        .args([
            "diff",
            "--cached",
            "--binary",
            diff_base,
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

    // `out.stdout` is owned (into memory) before the temp index is dropped &
    // cleaned up below, so the captured patch survives the cleanup.
    drop(tmp_index_dir);

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

impl JsonlProgressEmitter {
    /// Build the JSONL line for a progress event, or `None` if the event is not
    /// surfaced in stream-json. Split out from `emit` so it is unit-testable
    /// without capturing stdout.
    fn event_json_line(event: ProgressEvent) -> Option<String> {
        match event {
            ProgressEvent::StepStarted { step, model, .. } => {
                serde_json::to_string(&HeadlessEvent::step_started(step, model)).ok()
            }
            ProgressEvent::ToolCallStarted { tool, args_short } => {
                serde_json::to_string(&HeadlessEvent::tool_call_started(tool, args_short)).ok()
            }
            ProgressEvent::ToolCallCompleted { tool, ok, .. } => {
                serde_json::to_string(&HeadlessEvent::tool_call_completed(tool, ok)).ok()
            }
            ProgressEvent::StepCompleted { step, .. } => {
                serde_json::to_string(&HeadlessEvent::step_completed(step)).ok()
            }
            ProgressEvent::TaskCompleted { outcome } => {
                serde_json::to_string(&HeadlessEvent::task_completed(outcome)).ok()
            }
            ProgressEvent::TaskFailed { reason } => {
                serde_json::to_string(&HeadlessEvent::task_failed(reason)).ok()
            }
            ProgressEvent::TurnDecision { decision, detail } => {
                serde_json::to_string(&HeadlessEvent::turn_decision(decision, detail)).ok()
            }
            ProgressEvent::LlmRequestSent { tokens } => Some(
                serde_json::json!({
                    "event": "llm_request_sent",
                    "prompt_tokens": tokens,
                })
                .to_string(),
            ),
            ProgressEvent::LlmResponseReceived {
                finish_reason,
                completion_tokens,
                elapsed_ms,
            } => Some(
                serde_json::json!({
                    "event": "llm_response_received",
                    "finish_reason": finish_reason,
                    "completion_tokens": completion_tokens,
                    "elapsed_ms": elapsed_ms,
                })
                .to_string(),
            ),
            _ => None,
        }
    }
}

/// Guard the JSONL stream: refuse to write a line that is not self-contained
/// valid JSON. stdout under `--output-format stream-json` is a
/// machine-readable stream — a single partial or malformed line would
/// silently break every downstream JSONL consumer (2026-09-22 container e2e
/// finding). Failing LOUDLY on stderr instead of emitting the corrupt line
/// keeps the rest of the stream parseable.
///
/// Every line this module produces is valid by construction (`event_json_line`
/// / `emit_result` serialize with serde), so the guard is a tripwire for a
/// future bug, not a recovery path: when it fires the line is dropped and the
/// defect is called out rather than corrupting the stream.
fn validated_jsonl_line(line: String) -> Option<String> {
    match serde_json::from_str::<serde_json::Value>(&line) {
        Ok(_) => Some(line),
        Err(e) => {
            eprintln!(
                "stream-json internal error: refusing to emit non-JSON line ({}); \
                 the event is dropped to protect the stream",
                e
            );
            None
        }
    }
}

impl ProgressEmitter for JsonlProgressEmitter {
    fn emit(&self, event: ProgressEvent) {
        if let Some(line) = Self::event_json_line(event).and_then(validated_jsonl_line) {
            let _guard = self.lock.lock().unwrap_or_else(|e| e.into_inner());
            let mut stdout = std::io::stdout().lock();
            let _ = writeln!(stdout, "{}", line);
            let _ = stdout.flush();
        }
    }
}

#[cfg(test)]
#[path = "../../tests/unit/cli/headless/headless_test.rs"]
mod tests;
