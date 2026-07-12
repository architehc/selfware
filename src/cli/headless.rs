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
            outcome: if detail.is_empty() { None } else { Some(detail) },
            reason: Some(decision),
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

    let _ = std::process::Command::new("git")
        .env("GIT_INDEX_FILE", &tmp_index)
        .args(["add", "-A"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();

    let out = std::process::Command::new("git")
        .env("GIT_INDEX_FILE", &tmp_index)
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
            } => Some(
                serde_json::json!({
                    "event": "llm_response_received",
                    "finish_reason": finish_reason,
                    "completion_tokens": completion_tokens,
                })
                .to_string(),
            ),
            _ => None,
        }
    }
}

impl ProgressEmitter for JsonlProgressEmitter {
    fn emit(&self, event: ProgressEvent) {
        if let Some(line) = Self::event_json_line(event) {
            let _guard = self.lock.lock().unwrap_or_else(|e| e.into_inner());
            let mut stdout = std::io::stdout().lock();
            let _ = writeln!(stdout, "{}", line);
            let _ = stdout.flush();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::progress::ProgressEvent;
    use crate::observability::dashboard::TokenUsage;
    use serde_json::Value;
    use std::sync::Mutex;

    /// Mutex to serialize tests that change the working directory.
    static CWD_MUTEX: Mutex<()> = Mutex::new(());

    // ── SessionResult serialization ──────────────────────────────────────

    #[test]
    fn test_session_result_round_trip() {
        let result = SessionResult {
            session_id: "test-session-123".to_string(),
            exit_status: 0,
            stop_reason: "completed".to_string(),
            num_turns: 5,
            patch_bytes: 1024,
            patch_lines: 42,
            usage: TokenUsage::new(1000, 500),
            model: "test-model".to_string(),
            duration_ms: 30000,
            failure_mode: None,
            artifact_dir: Some(PathBuf::from("/tmp/artifacts")),
        };
        let json = serde_json::to_string(&result).unwrap();
        let de: SessionResult = serde_json::from_str(&json).unwrap();
        assert_eq!(de.session_id, "test-session-123");
        assert_eq!(de.exit_status, 0);
        assert_eq!(de.stop_reason, "completed");
        assert_eq!(de.num_turns, 5);
        assert_eq!(de.patch_bytes, 1024);
        assert_eq!(de.patch_lines, 42);
        assert_eq!(de.usage.input, 1000);
        assert_eq!(de.usage.output, 500);
        assert_eq!(de.usage.total, 1500);
        assert_eq!(de.model, "test-model");
        assert_eq!(de.duration_ms, 30000);
        assert!(de.failure_mode.is_none());
        assert_eq!(de.artifact_dir, Some(PathBuf::from("/tmp/artifacts")));
    }

    #[test]
    fn test_session_result_with_failure_mode() {
        let result = SessionResult {
            session_id: "fail-session".to_string(),
            exit_status: 1,
            stop_reason: "error".to_string(),
            num_turns: 3,
            patch_bytes: 0,
            patch_lines: 0,
            usage: TokenUsage::default(),
            model: "model-x".to_string(),
            duration_ms: 5000,
            failure_mode: Some("timeout".to_string()),
            artifact_dir: None,
        };
        let json = serde_json::to_string(&result).unwrap();
        let de: SessionResult = serde_json::from_str(&json).unwrap();
        assert_eq!(de.failure_mode, Some("timeout".to_string()));
        assert!(de.artifact_dir.is_none());
        assert_eq!(de.usage.total, 0);
        assert_eq!(de.exit_status, 1);
    }

    #[test]
    fn test_session_result_json_fields() {
        let result = SessionResult {
            session_id: "s1".to_string(),
            exit_status: 2,
            stop_reason: "stopped".to_string(),
            num_turns: 10,
            patch_bytes: 2048,
            patch_lines: 88,
            usage: TokenUsage::new(100, 200),
            model: "m1".to_string(),
            duration_ms: 60000,
            failure_mode: Some("loop_guard".to_string()),
            artifact_dir: Some(PathBuf::from("/out")),
        };
        let json = serde_json::to_string(&result).unwrap();
        let v: Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["session_id"], "s1");
        assert_eq!(v["exit_status"], 2);
        assert_eq!(v["stop_reason"], "stopped");
        assert_eq!(v["num_turns"], 10);
        assert_eq!(v["patch_bytes"], 2048);
        assert_eq!(v["patch_lines"], 88);
        assert_eq!(v["model"], "m1");
        assert_eq!(v["duration_ms"], 60000);
        assert_eq!(v["usage"]["input"], 100);
        assert_eq!(v["usage"]["output"], 200);
        assert_eq!(v["usage"]["total"], 300);
        assert_eq!(v["failure_mode"], "loop_guard");
        assert_eq!(v["artifact_dir"], "/out");
    }

    // ── HeadlessEvent constructors ───────────────────────────────────────

    #[test]
    fn test_step_started_constructor() {
        let ev = HeadlessEvent::step_started(3, "gpt-4".to_string());
        assert_eq!(ev.event, "step_started");
        assert_eq!(ev.step, Some(3));
        assert_eq!(ev.model.as_deref(), Some("gpt-4"));
        assert!(ev.tool.is_none());
        assert!(ev.args.is_none());
        assert!(ev.ok.is_none());
        assert!(ev.outcome.is_none());
        assert!(ev.reason.is_none());
    }

    #[test]
    fn test_tool_call_started_constructor() {
        let ev =
            HeadlessEvent::tool_call_started("file_read".to_string(), "path=foo.rs".to_string());
        assert_eq!(ev.event, "tool_call_started");
        assert_eq!(ev.tool.as_deref(), Some("file_read"));
        assert_eq!(ev.args.as_deref(), Some("path=foo.rs"));
        assert!(ev.step.is_none());
        assert!(ev.model.is_none());
        assert!(ev.ok.is_none());
        assert!(ev.outcome.is_none());
        assert!(ev.reason.is_none());
    }

    #[test]
    fn test_tool_call_completed_constructor_true() {
        let ev = HeadlessEvent::tool_call_completed("file_write".to_string(), true);
        assert_eq!(ev.event, "tool_call_completed");
        assert_eq!(ev.tool.as_deref(), Some("file_write"));
        assert_eq!(ev.ok, Some(true));
        assert!(ev.args.is_none());
        assert!(ev.step.is_none());
        assert!(ev.model.is_none());
        assert!(ev.outcome.is_none());
        assert!(ev.reason.is_none());
    }

    #[test]
    fn test_tool_call_completed_constructor_false() {
        let ev = HeadlessEvent::tool_call_completed("shell_exec".to_string(), false);
        assert_eq!(ev.ok, Some(false));
        assert_eq!(ev.tool.as_deref(), Some("shell_exec"));
    }

    #[test]
    fn jsonl_emits_llm_events_with_tokens_and_finish_reason() {
        // stream-json previously dropped the LLM events entirely.
        let req = JsonlProgressEmitter::event_json_line(ProgressEvent::LlmRequestSent {
            tokens: 1234,
        })
        .expect("llm_request_sent must be emitted");
        let req: Value = serde_json::from_str(&req).unwrap();
        assert_eq!(req["event"], "llm_request_sent");
        assert_eq!(req["prompt_tokens"], 1234);

        let resp = JsonlProgressEmitter::event_json_line(ProgressEvent::LlmResponseReceived {
            finish_reason: "stop".to_string(),
            completion_tokens: 56,
        })
        .expect("llm_response_received must be emitted");
        let resp: Value = serde_json::from_str(&resp).unwrap();
        assert_eq!(resp["event"], "llm_response_received");
        assert_eq!(resp["finish_reason"], "stop");
        assert_eq!(resp["completion_tokens"], 56);
    }

    #[test]
    fn test_step_completed_constructor() {
        let ev = HeadlessEvent::step_completed(7);
        assert_eq!(ev.event, "step_completed");
        assert_eq!(ev.step, Some(7));
        assert!(ev.model.is_none());
        assert!(ev.tool.is_none());
        assert!(ev.args.is_none());
        assert!(ev.ok.is_none());
        assert!(ev.outcome.is_none());
        assert!(ev.reason.is_none());
    }

    #[test]
    fn test_task_completed_constructor() {
        let ev = HeadlessEvent::task_completed("success".to_string());
        assert_eq!(ev.event, "task_completed");
        assert_eq!(ev.outcome.as_deref(), Some("success"));
        assert!(ev.reason.is_none());
        assert!(ev.step.is_none());
        assert!(ev.tool.is_none());
    }

    #[test]
    fn test_task_failed_constructor() {
        let ev = HeadlessEvent::task_failed("compilation error".to_string());
        assert_eq!(ev.event, "task_failed");
        assert_eq!(ev.reason.as_deref(), Some("compilation error"));
        assert!(ev.outcome.is_none());
        assert!(ev.tool.is_none());
        assert!(ev.step.is_none());
    }

    // ── HeadlessEvent serialization ──────────────────────────────────────

    #[test]
    fn test_step_started_serialization() {
        let ev = HeadlessEvent::step_started(1, "model-a".to_string());
        let json = serde_json::to_string(&ev).unwrap();
        let v: Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["event"], "step_started");
        assert_eq!(v["step"], 1);
        assert_eq!(v["model"], "model-a");
    }

    #[test]
    fn test_tool_call_started_serialization() {
        let ev = HeadlessEvent::tool_call_started("cargo_check".to_string(), "".to_string());
        let json = serde_json::to_string(&ev).unwrap();
        let v: Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["event"], "tool_call_started");
        assert_eq!(v["tool"], "cargo_check");
        assert_eq!(v["args"], "");
    }

    #[test]
    fn test_tool_call_completed_serialization() {
        let ev = HeadlessEvent::tool_call_completed("file_edit".to_string(), false);
        let json = serde_json::to_string(&ev).unwrap();
        let v: Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["event"], "tool_call_completed");
        assert_eq!(v["tool"], "file_edit");
        assert_eq!(v["ok"], false);
    }

    #[test]
    fn test_step_completed_serialization() {
        let ev = HeadlessEvent::step_completed(42);
        let json = serde_json::to_string(&ev).unwrap();
        let v: Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["event"], "step_completed");
        assert_eq!(v["step"], 42);
    }

    #[test]
    fn test_task_completed_serialization() {
        let ev = HeadlessEvent::task_completed("all tests passed".to_string());
        let json = serde_json::to_string(&ev).unwrap();
        let v: Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["event"], "task_completed");
        assert_eq!(v["outcome"], "all tests passed");
    }

    #[test]
    fn test_task_failed_serialization() {
        let ev = HeadlessEvent::task_failed("out of budget".to_string());
        let json = serde_json::to_string(&ev).unwrap();
        let v: Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["event"], "task_failed");
        assert_eq!(v["reason"], "out of budget");
    }

    #[test]
    fn test_skip_serializing_none_fields() {
        // step_completed only has event and step set — all other fields are
        // None and should be omitted from JSON via skip_serializing_if.
        let ev = HeadlessEvent::step_completed(1);
        let json = serde_json::to_string(&ev).unwrap();
        assert!(!json.contains("\"tool\""));
        assert!(!json.contains("\"model\""));
        assert!(!json.contains("\"args\""));
        assert!(!json.contains("\"ok\""));
        assert!(!json.contains("\"outcome\""));
        assert!(!json.contains("\"reason\""));
    }

    #[test]
    fn test_skip_serializing_none_fields_task_failed() {
        // task_failed has event and reason set — step/model/tool/args/ok/outcome
        // should all be skipped.
        let ev = HeadlessEvent::task_failed("err".to_string());
        let json = serde_json::to_string(&ev).unwrap();
        assert!(!json.contains("\"step\""));
        assert!(!json.contains("\"model\""));
        assert!(!json.contains("\"tool\""));
        assert!(!json.contains("\"args\""));
        assert!(!json.contains("\"ok\""));
        assert!(!json.contains("\"outcome\""));
        assert!(json.contains("\"reason\""));
    }

    // ── emit_event / emit_result (smoke tests — verify no panic) ──────────

    #[test]
    fn test_emit_event_does_not_panic() {
        let ev = HeadlessEvent::step_started(1, "test".to_string());
        emit_event(&ev);
    }

    #[test]
    fn test_emit_result_does_not_panic() {
        let result = SessionResult {
            session_id: "emit-test".to_string(),
            exit_status: 0,
            stop_reason: "done".to_string(),
            num_turns: 1,
            patch_bytes: 0,
            patch_lines: 0,
            usage: TokenUsage::default(),
            model: "test".to_string(),
            duration_ms: 0,
            failure_mode: None,
            artifact_dir: None,
        };
        emit_result(&result);
    }

    // ── JsonlProgressEmitter ─────────────────────────────────────────────

    #[test]
    fn test_jsonl_emitter_new_and_default() {
        let _emitter = JsonlProgressEmitter::new();
        let _default = JsonlProgressEmitter::default();
    }

    #[test]
    fn test_jsonl_emitter_step_started() {
        let emitter = JsonlProgressEmitter::new();
        emitter.emit(ProgressEvent::StepStarted {
            step: 1,
            model: "test-model".to_string(),
            tools_available: 5,
        });
    }

    #[test]
    fn test_jsonl_emitter_tool_call_started() {
        let emitter = JsonlProgressEmitter::new();
        emitter.emit(ProgressEvent::ToolCallStarted {
            tool: "file_read".to_string(),
            args_short: "path=test.rs".to_string(),
        });
    }

    #[test]
    fn test_jsonl_emitter_tool_call_completed() {
        let emitter = JsonlProgressEmitter::new();
        emitter.emit(ProgressEvent::ToolCallCompleted {
            tool: "file_read".to_string(),
            ok: true,
            elapsed_ms: 42,
        });
    }

    #[test]
    fn test_jsonl_emitter_step_completed() {
        let emitter = JsonlProgressEmitter::new();
        emitter.emit(ProgressEvent::StepCompleted {
            step: 3,
            mutating_tools_so_far: 2,
        });
    }

    #[test]
    fn test_jsonl_emitter_task_completed() {
        let emitter = JsonlProgressEmitter::new();
        emitter.emit(ProgressEvent::TaskCompleted {
            outcome: "success".to_string(),
        });
    }

    #[test]
    fn test_jsonl_emitter_task_failed() {
        let emitter = JsonlProgressEmitter::new();
        emitter.emit(ProgressEvent::TaskFailed {
            reason: "something went wrong".to_string(),
        });
    }

    #[test]
    fn test_jsonl_emitter_ignores_unmapped_events() {
        // Events that don't map to a HeadlessEvent variant should be silently
        // dropped (the `_ => None` arm).
        let emitter = JsonlProgressEmitter::new();
        emitter.emit(ProgressEvent::LlmRequestSent { tokens: 100 });
        emitter.emit(ProgressEvent::LlmResponseReceived {
            finish_reason: "stop".to_string(),
            completion_tokens: 50,
        });
        emitter.emit(ProgressEvent::GuardFired {
            kind: "progress".to_string(),
            count: 1,
        });
        emitter.emit(ProgressEvent::SubprocessStarted {
            name: "cargo".to_string(),
        });
        emitter.emit(ProgressEvent::SubprocessCompleted {
            name: "cargo".to_string(),
            exit: 0,
            elapsed_ms: 100,
        });
    }

    #[test]
    fn test_jsonl_emitter_simulated_agent_loop() {
        // Emit a realistic sequence of events as a real agent loop would.
        let emitter = JsonlProgressEmitter::new();
        emitter.emit(ProgressEvent::StepStarted {
            step: 1,
            model: "m".to_string(),
            tools_available: 3,
        });
        emitter.emit(ProgressEvent::ToolCallStarted {
            tool: "file_read".to_string(),
            args_short: "path=foo".to_string(),
        });
        emitter.emit(ProgressEvent::ToolCallCompleted {
            tool: "file_read".to_string(),
            ok: true,
            elapsed_ms: 10,
        });
        emitter.emit(ProgressEvent::StepCompleted {
            step: 1,
            mutating_tools_so_far: 0,
        });
        emitter.emit(ProgressEvent::TaskCompleted {
            outcome: "done".to_string(),
        });
    }

    // ── capture_patch ────────────────────────────────────────────────────

    fn git_available() -> bool {
        std::process::Command::new("git")
            .arg("--version")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .is_ok()
    }

    /// Helper: create a temp git repo with an initial commit and return its path.
    fn make_temp_git_repo(prefix: &str) -> Option<std::path::PathBuf> {
        if !git_available() {
            return None;
        }

        let tmp_dir = std::env::temp_dir().join(format!(
            "{}_{}_{}",
            prefix,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&tmp_dir).unwrap();

        // Init
        std::process::Command::new("git")
            .args(["init"])
            .current_dir(&tmp_dir)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .unwrap();

        // Configure user (required for commit)
        for (key, val) in &[("user.email", "t@t.com"), ("user.name", "T")] {
            std::process::Command::new("git")
                .args(["config", key, val])
                .current_dir(&tmp_dir)
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status()
                .unwrap();
        }

        // Initial file + commit
        std::fs::write(tmp_dir.join("file.txt"), "line1\nline2\n").unwrap();
        std::process::Command::new("git")
            .args(["add", "-A"])
            .current_dir(&tmp_dir)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .unwrap();
        std::process::Command::new("git")
            .args(["commit", "-m", "initial"])
            .current_dir(&tmp_dir)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .unwrap();

        Some(tmp_dir)
    }

    struct TempDirCleanup(std::path::PathBuf);
    impl Drop for TempDirCleanup {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn test_capture_patch_detects_modified_and_new_files() {
        let tmp_dir = match make_temp_git_repo("selfware_cp_mod") {
            Some(d) => d,
            None => {
                eprintln!("Skipping: git not available");
                return;
            }
        };
        let _cleanup = TempDirCleanup(tmp_dir.clone());
        let _guard = CWD_MUTEX.lock().unwrap_or_else(|e| e.into_inner());
        let original_dir = std::env::current_dir().unwrap();

        // Modify existing file
        std::fs::write(tmp_dir.join("file.txt"), "line1\nline2\nline3\n").unwrap();
        // Add new file
        std::fs::write(tmp_dir.join("new.txt"), "new content\n").unwrap();

        std::env::set_current_dir(&tmp_dir).unwrap();
        let result = capture_patch();
        std::env::set_current_dir(&original_dir).unwrap();

        assert!(result.is_ok(), "capture_patch failed: {:?}", result.err());
        let patch = result.unwrap();
        assert!(
            patch.contains("line3"),
            "patch should contain new line3, got: {}",
            patch
        );
        assert!(
            patch.contains("new content"),
            "patch should contain new file, got: {}",
            patch
        );
    }

    #[test]
    fn test_capture_patch_empty_when_no_changes() {
        let tmp_dir = match make_temp_git_repo("selfware_cp_empty") {
            Some(d) => d,
            None => {
                eprintln!("Skipping: git not available");
                return;
            }
        };
        let _cleanup = TempDirCleanup(tmp_dir.clone());
        let _guard = CWD_MUTEX.lock().unwrap_or_else(|e| e.into_inner());
        let original_dir = std::env::current_dir().unwrap();

        std::env::set_current_dir(&tmp_dir).unwrap();
        let result = capture_patch();
        std::env::set_current_dir(&original_dir).unwrap();

        assert!(result.is_ok(), "capture_patch failed: {:?}", result.err());
        let patch = result.unwrap();
        assert!(
            patch.trim().is_empty(),
            "patch should be empty with no changes, got: {:?}",
            patch
        );
    }

    #[test]
    fn test_capture_patch_excludes_internal_dirs() {
        let tmp_dir = match make_temp_git_repo("selfware_cp_excl") {
            Some(d) => d,
            None => {
                eprintln!("Skipping: git not available");
                return;
            }
        };
        let _cleanup = TempDirCleanup(tmp_dir.clone());
        let _guard = CWD_MUTEX.lock().unwrap_or_else(|e| e.into_inner());
        let original_dir = std::env::current_dir().unwrap();

        // Modify main file (should appear)
        std::fs::write(tmp_dir.join("file.txt"), "line1\nline2\nCHANGED\n").unwrap();

        // Create files in excluded directories
        std::fs::create_dir_all(tmp_dir.join(".selfware")).unwrap();
        std::fs::write(tmp_dir.join(".selfware/cache.txt"), "secret cache\n").unwrap();

        std::env::set_current_dir(&tmp_dir).unwrap();
        let result = capture_patch();
        std::env::set_current_dir(&original_dir).unwrap();

        assert!(result.is_ok(), "capture_patch failed: {:?}", result.err());
        let patch = result.unwrap();
        assert!(
            patch.contains("CHANGED"),
            "patch should contain real change, got: {}",
            patch
        );
        assert!(
            !patch.contains("secret cache"),
            "patch should exclude .selfware/, got: {}",
            patch
        );
    }

    #[test]
    fn test_capture_patch_does_not_stage_user_files() {
        let tmp_dir = match make_temp_git_repo("selfware_cp_nostage") {
            Some(d) => d,
            None => {
                eprintln!("Skipping: git not available");
                return;
            }
        };
        let _cleanup = TempDirCleanup(tmp_dir.clone());
        let _guard = CWD_MUTEX.lock().unwrap_or_else(|e| e.into_inner());
        let original_dir = std::env::current_dir().unwrap();

        // Make an unstaged modification to the tracked file.
        std::fs::write(tmp_dir.join("file.txt"), "line1\nline2\nUNSTAGED\n").unwrap();

        // Snapshot the real index state BEFORE capture_patch (git diff --cached
        // against the real index should be empty since nothing is staged yet).
        std::env::set_current_dir(&tmp_dir).unwrap();
        let staged_before = std::process::Command::new("git")
            .args(["diff", "--cached", "HEAD"])
            .output()
            .unwrap();
        std::env::set_current_dir(&original_dir).unwrap();
        let staged_before = String::from_utf8_lossy(&staged_before.stdout).to_string();

        // Run capture_patch (should NOT touch the real index).
        std::env::set_current_dir(&tmp_dir).unwrap();
        let result = capture_patch();
        std::env::set_current_dir(&original_dir).unwrap();

        assert!(result.is_ok(), "capture_patch failed: {:?}", result.err());
        let patch = result.unwrap();
        assert!(
            patch.contains("UNSTAGED"),
            "patch should contain the unstaged change, got: {}",
            patch
        );

        // Snapshot the real index state AFTER capture_patch and compare. The
        // real `.git/index` must be untouched, so `git diff --cached` must be
        // identical to the before snapshot.
        std::env::set_current_dir(&tmp_dir).unwrap();
        let staged_after = std::process::Command::new("git")
            .args(["diff", "--cached", "HEAD"])
            .output()
            .unwrap();
        std::env::set_current_dir(&original_dir).unwrap();
        let staged_after = String::from_utf8_lossy(&staged_after.stdout).to_string();

        assert_eq!(
            staged_before, staged_after,
            "capture_patch must not mutate the user's real git index (staged state changed)"
        );
    }
}
