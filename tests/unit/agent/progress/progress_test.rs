use super::*;

#[test]
fn render_step_started_kv() {
    let ev = ProgressEvent::StepStarted {
        step: 5,
        model: "test-model".into(),
        tools_available: 12,
    };
    let s = render_event_kv(&ev);
    assert!(s.contains("kind=step_started"));
    assert!(s.contains("step=5"));
    assert!(s.contains("model=test-model"));
    assert!(s.contains("tools_available=12"));
}

#[test]
fn render_tool_call_completed_kv() {
    let ev = ProgressEvent::ToolCallCompleted {
        tool: "file_read".into(),
        ok: true,
        elapsed_ms: 234,
    };
    let s = render_event_kv(&ev);
    assert_eq!(s, "kind=tool_call_completed tool=file_read ok=true 234ms");
}

#[test]
fn multi_emitter_fans_out() {
    let a = Arc::new(RecordingProgressEmitter::new());
    let b = Arc::new(RecordingProgressEmitter::new());
    let multi = MultiProgressEmitter::new(vec![a.clone(), b.clone()]);
    multi.emit(ProgressEvent::TaskCompleted {
        outcome: "ok".into(),
    });
    multi.emit(ProgressEvent::TaskFailed {
        reason: "oops".into(),
    });
    assert_eq!(a.snapshot().len(), 2);
    assert_eq!(b.snapshot().len(), 2);
}

#[test]
fn render_turn_decision_kv() {
    let ev = ProgressEvent::TurnDecision {
        decision: "refused".to_string(),
        detail: "completion gate: no edit made".to_string(),
    };
    assert_eq!(ev.kind(), "turn_decision");
    let s = render_event_kv(&ev);
    assert!(s.contains("kind=turn_decision"), "line: {s}");
    assert!(s.contains("decision=refused"), "line: {s}");
    assert!(s.contains("detail="), "line: {s}");
}

#[test]
fn render_turn_decision_kv_no_detail() {
    let ev = ProgressEvent::TurnDecision {
        decision: "no_tool_call".to_string(),
        detail: String::new(),
    };
    let s = render_event_kv(&ev);
    assert!(
        s.contains("kind=turn_decision decision=no_tool_call"),
        "line: {s}"
    );
    assert!(!s.contains("detail="), "line: {s}");
}

/// Simulates the events a small agent loop should emit and asserts they
/// arrive in the expected order. This is the contract that
/// [`crate::agent::Agent::run_task`] must honor.
#[test]
fn simulated_loop_event_order() {
    let rec = Arc::new(RecordingProgressEmitter::new());
    let em: Arc<dyn ProgressEmitter> = rec.clone();

    // Simulated loop:
    em.emit(ProgressEvent::StepStarted {
        step: 1,
        model: "m".into(),
        tools_available: 3,
    });
    em.emit(ProgressEvent::LlmRequestSent { tokens: 100 });
    em.emit(ProgressEvent::LlmResponseReceived {
        finish_reason: "tool_calls".into(),
        completion_tokens: 42,
    });
    em.emit(ProgressEvent::ToolCallStarted {
        tool: "file_read".into(),
        args_short: "path=foo".into(),
    });
    em.emit(ProgressEvent::ToolCallCompleted {
        tool: "file_read".into(),
        ok: true,
        elapsed_ms: 12,
    });
    em.emit(ProgressEvent::StepCompleted {
        step: 1,
        mutating_tools_so_far: 0,
    });
    em.emit(ProgressEvent::TaskCompleted {
        outcome: "success".into(),
    });

    assert_eq!(
        rec.kinds(),
        vec![
            "step_started",
            "llm_request_sent",
            "llm_response_received",
            "tool_call_started",
            "tool_call_completed",
            "step_completed",
            "task_completed",
        ]
    );
}

/// Asserts the contract for guard firing inside a step: the guard event
/// arrives between the tool batch completing and the step completing.
#[test]
fn simulated_loop_with_guard_fire() {
    let rec = Arc::new(RecordingProgressEmitter::new());
    let em: Arc<dyn ProgressEmitter> = rec.clone();

    em.emit(ProgressEvent::StepStarted {
        step: 7,
        model: "m".into(),
        tools_available: 2,
    });
    em.emit(ProgressEvent::ToolCallStarted {
        tool: "file_read".into(),
        args_short: String::new(),
    });
    em.emit(ProgressEvent::ToolCallCompleted {
        tool: "file_read".into(),
        ok: true,
        elapsed_ms: 5,
    });
    em.emit(ProgressEvent::GuardFired {
        kind: "progress_warning".into(),
        count: 1,
    });
    em.emit(ProgressEvent::StepCompleted {
        step: 7,
        mutating_tools_so_far: 0,
    });

    let kinds = rec.kinds();
    let guard_idx = kinds.iter().position(|k| *k == "guard_fired").unwrap();
    let step_done_idx = kinds.iter().position(|k| *k == "step_completed").unwrap();
    let tool_done_idx = kinds
        .iter()
        .position(|k| *k == "tool_call_completed")
        .unwrap();
    assert!(tool_done_idx < guard_idx);
    assert!(guard_idx < step_done_idx);
}
