use super::*;
use serde_json::json;

#[test]
fn trace_roundtrip() {
    let mut trace = RunTrace::new("r1".into(), "i1".into(), "q1".into(), 1);
    trace.emit(TraceEvent::LlmRequest {
        step: 1,
        estimated_tokens: 100,
    });
    trace.emit(TraceEvent::ToolCallStarted {
        step: 1,
        tool: "file_read".into(),
        args: json!("path=foo"),
    });
    trace.emit(TraceEvent::ToolCallCompleted {
        step: 1,
        tool: "file_read".into(),
        success: true,
        duration_ms: 12,
    });
    trace.emit(TraceEvent::PatchCaptured {
        patch_lines: 5,
        patch_bytes: 120,
    });

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("trace.jsonl");
    trace.write_jsonl(&path).unwrap();

    let loaded = RunTrace::read_jsonl(&path).unwrap();
    assert_eq!(loaded.events.len(), 4);
    assert_eq!(
        loaded.events[0],
        TraceEvent::LlmRequest {
            step: 1,
            estimated_tokens: 100
        }
    );
    assert_eq!(
        loaded.events[1],
        TraceEvent::ToolCallStarted {
            step: 1,
            tool: "file_read".into(),
            args: json!("path=foo")
        }
    );
}

#[test]
fn diagnosis_histogram() {
    let mut trace1 = RunTrace::new("r1".into(), "i1".into(), "q1".into(), 1);
    trace1.emit(TraceEvent::FailureClassified {
        kind: "FakeComplete".into(),
        evidence: "ev".into(),
    });
    trace1.emit(TraceEvent::GuardFired {
        kind: "progress".into(),
        count: 1,
    });
    trace1.emit(TraceEvent::ToolCallStarted {
        step: 2,
        tool: "file_write".into(),
        args: json!(""),
    });
    trace1.emit(TraceEvent::ToolCallCompleted {
        step: 2,
        tool: "file_write".into(),
        success: true,
        duration_ms: 10,
    });

    let mut trace2 = RunTrace::new("r2".into(), "i2".into(), "q1".into(), 1);
    trace2.emit(TraceEvent::FailureClassified {
        kind: "Timeout".into(),
        evidence: "ev".into(),
    });
    trace2.emit(TraceEvent::ToolCallCompleted {
        step: 1,
        tool: "file_read".into(),
        success: false,
        duration_ms: 5,
    });

    let d1 = PerRunDiagnosis::from_trace(&trace1);
    let d2 = PerRunDiagnosis::from_trace(&trace2);

    assert!(d1.fake_complete);
    assert!(!d1.timeout);
    assert!(d2.timeout);
    assert_eq!(d1.total_tool_calls, 1);
    assert_eq!(d2.syntax_failures, 1);

    let summary = DiagnosisSummary::from_diagnoses(&[(trace1, d1), (trace2, d2)]);
    assert_eq!(summary.total_runs, 2);
    assert_eq!(
        summary.failure_mode_histogram.get("FakeComplete").copied(),
        Some(1)
    );
    assert_eq!(
        summary.failure_mode_histogram.get("Timeout").copied(),
        Some(1)
    );
    assert_eq!(summary.median_turns_to_first_edit, 2.0);
    assert!((summary.fake_complete_rate - 0.5).abs() < 1e-9);
    assert!((summary.timeout_rate - 0.5).abs() < 1e-9);
}
