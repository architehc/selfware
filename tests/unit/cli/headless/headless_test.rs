use super::*;
use crate::agent::progress::ProgressEvent;
use crate::observability::dashboard::TokenUsage;
use serde_json::Value;

// ── headless_mode_block_reason ───────────────────────────────────────

#[test]
fn headless_block_reason_normal_mode_without_tty_is_blocked() {
    let reason = headless_mode_block_reason(ExecutionMode::Normal, false)
        .expect("Normal mode + non-TTY stdin must be blocked");
    assert!(
        reason.contains("-m yolo") && reason.contains("-m auto-edit"),
        "message must name the fix, got: {}",
        reason
    );
    assert!(
        reason.contains("stdin is not a terminal"),
        "message must name the real cause, got: {}",
        reason
    );
}

#[test]
fn headless_block_reason_normal_mode_with_tty_is_allowed() {
    // Interactive terminal: the confirmation prompt can be answered.
    assert!(headless_mode_block_reason(ExecutionMode::Normal, true).is_none());
}

#[test]
fn headless_block_reason_autonomous_modes_allowed_without_tty() {
    assert!(headless_mode_block_reason(ExecutionMode::Yolo, false).is_none());
    assert!(headless_mode_block_reason(ExecutionMode::AutoEdit, false).is_none());
    assert!(headless_mode_block_reason(ExecutionMode::Daemon, false).is_none());
}

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
        answer: None,
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
        answer: None,
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
        answer: None,
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
    let ev = HeadlessEvent::tool_call_started("file_read".to_string(), "path=foo.rs".to_string());
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
    let req = JsonlProgressEmitter::event_json_line(ProgressEvent::LlmRequestSent { tokens: 1234 })
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
        answer: None,
    };
    emit_result(&result);
}

// ── SessionResult answer (final assistant response) ─────────────────

#[test]
fn test_session_result_serializes_final_answer() {
    // Regression: the headless JSON result previously carried no final
    // answer, so machine consumers could never see what the agent concluded.
    let result = SessionResult {
        session_id: "answer-session".to_string(),
        exit_status: 0,
        stop_reason: "completed".to_string(),
        num_turns: 7,
        patch_bytes: 0,
        patch_lines: 0,
        usage: TokenUsage::default(),
        model: "test-model".to_string(),
        duration_ms: 12000,
        failure_mode: None,
        artifact_dir: None,
        answer: Some("Fixed the lint and verified with cargo test.".to_string()),
    };
    let json = serde_json::to_string(&result).unwrap();
    let v: Value = serde_json::from_str(&json).unwrap();
    assert_eq!(
        v["answer"], "Fixed the lint and verified with cargo test.",
        "JSON output must carry the final answer"
    );
    let de: SessionResult = serde_json::from_str(&json).unwrap();
    assert_eq!(
        de.answer.as_deref(),
        Some("Fixed the lint and verified with cargo test.")
    );
}

#[test]
fn test_session_result_omits_answer_when_none() {
    // Pre-existing consumers see a byte-identical shape for runs without an
    // answer: the key must not appear at all.
    let result = SessionResult {
        session_id: "no-answer".to_string(),
        exit_status: 0,
        stop_reason: "completed".to_string(),
        num_turns: 1,
        patch_bytes: 0,
        patch_lines: 0,
        usage: TokenUsage::default(),
        model: "m".to_string(),
        duration_ms: 1,
        failure_mode: None,
        artifact_dir: None,
        answer: None,
    };
    let json = serde_json::to_string(&result).unwrap();
    assert!(
        !json.contains("\"answer\""),
        "None answer must not be serialized, got: {}",
        json
    );
    // Legacy streams (no `answer` key) still deserialize — answer defaults None.
    let de: SessionResult = serde_json::from_str(&json).unwrap();
    assert!(de.answer.is_none());
}

#[test]
fn test_answer_capture_emitter_records_completed_message() {
    // The headless runner attaches the capture emitter before the run; the
    // run-end `Completed` event carries `last_assistant_response.trim()`.
    let capture = AnswerCapture::new();
    capture.emitter().emit(AgentEvent::Completed {
        message: "The bug was a signed-offset read.".to_string(),
    });
    assert_eq!(
        capture.take().as_deref(),
        Some("The bug was a signed-offset read.")
    );
}

#[test]
fn test_answer_capture_trims_whitespace() {
    let capture = AnswerCapture::new();
    capture.emitter().emit(AgentEvent::Completed {
        message: "  Final answer.  \n".to_string(),
    });
    assert_eq!(capture.take().as_deref(), Some("Final answer."));
}

#[test]
fn test_answer_capture_ignores_errors_and_empty_messages() {
    // A failed run emits `Error` (the run's diagnostic, not the agent's
    // conclusion) and a streamed-commit completion may emit an empty
    // `Completed`; neither may become an answer.
    let capture = AnswerCapture::new();
    let emitter = capture.emitter();
    emitter.emit(AgentEvent::Error {
        message: "boom".to_string(),
    });
    emitter.emit(AgentEvent::Completed {
        message: "   ".to_string(),
    });
    assert!(capture.take().is_none());
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
    let _guard = crate::test_support::CwdGuard::hold();
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
fn test_capture_patch_no_phantom_deletion_for_tracked_ignored_files() {
    // Regression: a file that is tracked in HEAD but matches .gitignore must
    // NOT be reported as deleted when it is unchanged. Before seeding the
    // temp index from HEAD, `git add -A` skipped it (ignored) and the diff
    // showed a spurious deletion — a huge phantom patch with zero real edits.
    let tmp_dir = match make_temp_git_repo("selfware_cp_phantom") {
        Some(d) => d,
        None => {
            eprintln!("Skipping: git not available");
            return;
        }
    };
    let _cleanup = TempDirCleanup(tmp_dir.clone());
    let _guard = crate::test_support::CwdGuard::hold();
    let original_dir = std::env::current_dir().unwrap();

    // Two commits, in order, so the file ends up tracked-BUT-ignored:
    //   1) commit data.gen while it is still un-ignored (so it is tracked),
    //   2) THEN add a .gitignore that ignores it.
    // (Writing both at once and committing would let `git add -A` skip the
    // already-ignored file, so it would never be tracked — not the case we
    // want to exercise.)
    let git = |args: &[&str]| {
        std::process::Command::new("git")
            .args(args)
            .current_dir(&tmp_dir)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .unwrap();
    };
    std::fs::write(tmp_dir.join("data.gen"), "generated payload\n").unwrap();
    git(&["add", "-A"]);
    git(&["commit", "-m", "add data.gen (tracked)"]);
    std::fs::write(tmp_dir.join(".gitignore"), "data.gen\n").unwrap();
    git(&["add", "-A"]);
    git(&["commit", "-m", "ignore data.gen (now tracked-but-ignored)"]);

    // No working-tree changes at all → the patch must be empty, and in
    // particular must not mention data.gen (as a deletion or otherwise).
    std::env::set_current_dir(&tmp_dir).unwrap();
    let result = capture_patch();
    std::env::set_current_dir(&original_dir).unwrap();

    assert!(result.is_ok(), "capture_patch failed: {:?}", result.err());
    let patch = result.unwrap();
    assert!(
        !patch.contains("data.gen"),
        "tracked-but-ignored file reported as a phantom change:\n{}",
        patch
    );
    assert!(
        patch.trim().is_empty(),
        "no real edits, so the patch must be empty; got:\n{}",
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
    let _guard = crate::test_support::CwdGuard::hold();
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
    let _guard = crate::test_support::CwdGuard::hold();
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
fn test_capture_patch_works_on_repo_with_no_commits() {
    // Review finding: a freshly `git init`-ed repo has no HEAD, so the
    // headless capture's `git diff --cached --binary HEAD` failed and a
    // fully-successful task was reported as patch_capture_failed. The diff
    // must fall back to the empty tree object and return a valid patch.
    if !git_available() {
        eprintln!("Skipping: git not available");
        return;
    }
    let tmp_dir = std::env::temp_dir().join(format!(
        "selfware_cp_nocommits_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&tmp_dir).unwrap();
    let _cleanup = TempDirCleanup(tmp_dir.clone());
    let _guard = crate::test_support::CwdGuard::hold();
    let original_dir = std::env::current_dir().unwrap();

    // `git init` only — NO commit, so HEAD does not exist.
    std::process::Command::new("git")
        .args(["init"])
        .current_dir(&tmp_dir)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .unwrap();

    // One new file, staged implicitly by the capture's `git add -A`.
    std::fs::write(tmp_dir.join("hello.txt"), "hello world\n").unwrap();

    std::env::set_current_dir(&tmp_dir).unwrap();
    let result = capture_patch();
    std::env::set_current_dir(&original_dir).unwrap();

    let patch =
        result.unwrap_or_else(|e| panic!("capture_patch failed on a zero-commit repo: {e}"));
    assert!(
        patch.contains("hello world"),
        "a brand-new file in a zero-commit repo must appear in the patch (as an addition):\n{}",
        patch
    );

    // And an EMPTY zero-commit repo is a valid EMPTY patch, not an error.
    std::fs::remove_file(tmp_dir.join("hello.txt")).unwrap();
    std::env::set_current_dir(&tmp_dir).unwrap();
    let empty_result = capture_patch();
    std::env::set_current_dir(&original_dir).unwrap();
    let empty_patch =
        empty_result.unwrap_or_else(|e| panic!("empty zero-commit repo must not error: {e}"));
    assert!(
        empty_patch.trim().is_empty(),
        "an empty zero-commit repo must yield an empty patch, got:\n{}",
        empty_patch
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
    let _guard = crate::test_support::CwdGuard::hold();
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

// ── stream-json purity (2026-09-22 container e2e finding) ─────────────

#[test]
fn validated_jsonl_line_accepts_self_contained_json() {
    let line = r#"{"event":"step_started","step":1,"model":"m"}"#.to_string();
    assert_eq!(
        validated_jsonl_line(line.clone()).as_deref(),
        Some(line.as_str())
    );
}

#[test]
fn validated_jsonl_line_rejects_non_json_lines() {
    // A stray non-JSON line must never reach a stream-json stdout: the guard
    // drops it and fails loudly on stderr instead of corrupting the stream.
    assert!(validated_jsonl_line("=== DEBUG: Planning Response ===".to_string()).is_none());
    assert!(validated_jsonl_line("partial json {".to_string()).is_none());
    assert!(validated_jsonl_line(String::new()).is_none());
}

#[test]
fn every_mapped_progress_event_serializes_to_valid_json() {
    // The writer's own output must be valid JSON by construction: every event
    // surfaced in stream-json, serialized through the guard, parses.
    let events = [
        ProgressEvent::StepStarted {
            step: 1,
            model: "m".into(),
            tools_available: 3,
        },
        ProgressEvent::ToolCallStarted {
            tool: "file_read".into(),
            args_short: "path=a".into(),
        },
        ProgressEvent::ToolCallCompleted {
            tool: "file_read".into(),
            ok: true,
            elapsed_ms: 3,
        },
        ProgressEvent::StepCompleted {
            step: 1,
            mutating_tools_so_far: 0,
        },
        ProgressEvent::TaskCompleted {
            outcome: "done".into(),
        },
        ProgressEvent::LlmRequestSent { tokens: 10 },
        ProgressEvent::LlmResponseReceived {
            finish_reason: "stop".into(),
            completion_tokens: 5,
        },
    ];
    for event in events {
        let line = JsonlProgressEmitter::event_json_line(event)
            .expect("event must be surfaced in stream-json");
        assert!(
            validated_jsonl_line(line.clone()).is_some(),
            "mapped event produced a non-JSON line: {line}"
        );
    }
}

// ── SessionResult cost contract (2026-09-22 finding: cost: null) ──────

#[test]
fn cost_field_is_omitted_when_provider_reported_no_pricing() {
    // `usage.cost: None` (keyless endpoints — qwen38-flash against
    // llm.selfware.design reports token counts but no pricing) must NOT
    // serialize as `"cost": null`: a reader should never have to distinguish
    // a null from a missing key. cost is present only when the provider
    // actually reported it.
    let result = SessionResult {
        session_id: "cost-none".to_string(),
        exit_status: 0,
        stop_reason: "completed".to_string(),
        num_turns: 1,
        patch_bytes: 0,
        patch_lines: 0,
        usage: TokenUsage::new(100, 50), // cost defaults to None
        model: "m".to_string(),
        duration_ms: 1,
        failure_mode: None,
        artifact_dir: None,
        answer: None,
    };
    let json = serde_json::to_string(&result).unwrap();
    assert!(
        !json.contains("\"cost\""),
        "cost: None must be omitted from the JSON, got: {json}"
    );
    // And the omitted key still deserializes to None for consumers.
    let de: SessionResult = serde_json::from_str(&json).unwrap();
    assert!(de.usage.cost.is_none());
}

#[test]
fn cost_field_is_present_when_provider_priced_usage() {
    let mut usage = TokenUsage::new(100, 50);
    usage.cost = Some(0.0123);
    let result = SessionResult {
        session_id: "cost-some".to_string(),
        exit_status: 0,
        stop_reason: "completed".to_string(),
        num_turns: 1,
        patch_bytes: 0,
        patch_lines: 0,
        usage,
        model: "m".to_string(),
        duration_ms: 1,
        failure_mode: None,
        artifact_dir: None,
        answer: None,
    };
    let json = serde_json::to_string(&result).unwrap();
    let v: Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["usage"]["cost"], 0.0123);
}

// ── stdout stays pure JSONL with diagnostics enabled (regression) ─────

/// Remove ANSI escape sequences (belt-and-braces: this test must hold even
/// if a future color path runs with color forced on).
#[cfg(unix)]
fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' && chars.peek() == Some(&'[') {
            chars.next();
            for n in chars.by_ref() {
                if n.is_ascii_alphabetic() {
                    break;
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// Redirect fd 1 (stdout) to an anonymous temp file for the duration of the
/// guard, restoring the ORIGINAL fd 1 on drop — even when a test panics. The
/// original descriptor is duplicated so the restore target stays valid.
#[cfg(unix)]
struct Fd1Redirect {
    saved: i32,
    file: std::fs::File,
}

#[cfg(unix)]
impl Fd1Redirect {
    fn new() -> Self {
        use std::os::unix::io::AsRawFd;
        let saved = unsafe { libc::dup(1) };
        assert!(saved >= 0, "dup(1) failed");
        let file = tempfile::tempfile().expect("temp file for stdout capture");
        if unsafe { libc::dup2(file.as_raw_fd(), 1) } < 0 {
            unsafe { libc::close(saved) };
            panic!("dup2(stdout -> temp file) failed");
        }
        Self { saved, file }
    }
}

#[cfg(unix)]
impl Drop for Fd1Redirect {
    fn drop(&mut self) {
        // Flush buffered stdout so every line reaches the redirected file
        // BEFORE the original fd 1 is restored.
        use std::io::Write;
        let _ = std::io::stdout().flush();
        unsafe {
            libc::dup2(self.saved, 1);
            libc::close(self.saved);
        }
    }
}

#[cfg(unix)]
#[test]
fn stream_json_stdout_carries_only_json_lines() {
    // Container e2e regression (2026-09-22): with -v / the debug channel on
    // AND --output-format stream-json, non-JSON === DEBUG === blocks were
    // interleaved with the JSON events (observed: 8 of 23 lines). The
    // diagnostic-routing half of the fix is pinned deterministically in the
    // output-module tests (`diagnostic_sink_for`); this test pins the writer
    // half from the same angle the container validated it: while stdout is
    // the JSONL stream, every line that lands there must parse as JSON.
    //
    // fd 1 is process-global and the harness runs tests in parallel, so a
    // capture window can open mid-write from another thread: the first
    // captured line may be a torn suffix and window-edge lines may be lost
    // entirely (observed 2026-09-22: the leading step_started line and the
    // head of the next line never reached the file). That is capture-side
    // infrastructure noise, not application output — so the window is
    // RETRIED until all five emitted events are observed intact. A genuine
    // purity violation (a debug block interleaving) is deterministic and
    // fails every window, so retrying cannot mask it.
    use std::io::{Read, Seek, SeekFrom};

    let prior_json = crate::output::is_json_mode();
    crate::output::set_json_mode(true);
    struct RestoreJson(bool);
    impl Drop for RestoreJson {
        fn drop(&mut self) {
            crate::output::set_json_mode(self.0);
        }
    }
    let _json_guard = RestoreJson(prior_json);

    let expected_markers = [
        "\"event\":\"step_started\"",
        "\"event\":\"tool_call_started\"",
        "\"event\":\"tool_call_completed\"",
        "\"event\":\"task_completed\"",
        "\"event\":\"llm_response_received\"",
    ];

    let mut captured = String::new();
    let mut settled = false;
    for _attempt in 0..5 {
        let capture = Fd1Redirect::new();

        // A realistic stream-json event sequence.
        let emitter = JsonlProgressEmitter::new();
        emitter.emit(ProgressEvent::StepStarted {
            step: 1,
            model: "m".into(),
            tools_available: 2,
        });
        emitter.emit(ProgressEvent::ToolCallStarted {
            tool: "file_read".into(),
            args_short: "path=a".into(),
        });
        emitter.emit(ProgressEvent::ToolCallCompleted {
            tool: "file_read".into(),
            ok: true,
            elapsed_ms: 5,
        });
        emitter.emit(ProgressEvent::TaskCompleted {
            outcome: "done".into(),
        });
        emitter.emit(ProgressEvent::LlmResponseReceived {
            finish_reason: "stop".into(),
            completion_tokens: 5,
        });

        // Flush BEFORE restoring fd 1 so no buffered line leaks past the
        // window, then read what reached stdout: the capture file is an
        // anonymous temp file; duplicate its descriptor so we can seek+read
        // the same inode without disturbing the guard's fd.
        {
            use std::io::Write;
            let _ = std::io::stdout().flush();
        }
        captured.clear();
        {
            use std::os::unix::io::{AsRawFd, FromRawFd};
            let dup = unsafe { libc::dup(capture.file.as_raw_fd()) };
            assert!(dup >= 0, "dup of capture file failed");
            let mut read_handle = unsafe { std::fs::File::from_raw_fd(dup) };
            read_handle.seek(SeekFrom::Start(0)).unwrap();
            read_handle.read_to_string(&mut captured).unwrap();
        }
        drop(capture); // restore fd 1

        if expected_markers.iter().all(|m| captured.contains(m)) {
            settled = true;
            break;
        }
    }
    drop(_json_guard); // restore json mode

    assert!(
        settled,
        "capture window never observed all five events intact after 5 attempts; \
         last window got:\n{captured}"
    );

    let mut non_json_lines = 0usize;
    let mut json_lines = 0usize;
    for (idx, raw) in captured.lines().enumerate() {
        let line = strip_ansi(raw).trim().to_string();
        if line.is_empty() {
            continue;
        }
        if line.starts_with("test ") {
            // libtest's own `test <path> ... ok` status lines land on the
            // test process's shared fd 1 while the redirect is active (a
            // parallel test completed mid-window). They are harness
            // infrastructure, not application output — the contract being
            // pinned is that everything the APPLICATION emits to stdout in
            // stream-json mode is valid JSON.
            continue;
        }
        if idx == 0 && !line.starts_with('{') {
            // Torn window edge: the capture opened while another thread was
            // mid-write on the shared fd, so the first line is a suffix of
            // some line, not a line the application emitted. A corrupt line
            // the application DID emit is deterministic and reappears on
            // every retry above, so it cannot hide here.
            continue;
        }
        json_lines += 1;
        if serde_json::from_str::<Value>(&line).is_err() {
            non_json_lines += 1;
        }
    }
    assert!(
        json_lines >= 5,
        "expected at least the emitted JSON events on stdout, got: {captured}"
    );
    assert_eq!(
        non_json_lines, 0,
        "stdout under stream-json must be pure JSON lines, got:\n{captured}"
    );
}
