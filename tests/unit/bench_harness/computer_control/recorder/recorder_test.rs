use super::*;
use std::path::PathBuf;

#[test]
fn test_recorder_basic_flow() {
    let mut recorder =
        InteractionRecorder::new("test-1", "Test Task", PathBuf::from("/tmp/screenshots"));

    recorder.record_action(
        WebAction::Navigate {
            url: "https://example.com".into(),
        },
        ActionOutcome::Success {
            output: "200 OK".into(),
        },
        150,
        None,
        None,
    );

    recorder.record_screenshot(
        "after_nav",
        PathBuf::from("/tmp/screenshots/s1.png"),
        (1920, 1080),
    );

    let trace = recorder.finish(TaskOutcome::Passed);

    assert_eq!(trace.task_id, "test-1");
    assert_eq!(trace.actions.len(), 1);
    assert_eq!(trace.screenshots.len(), 1);
    assert!(matches!(trace.final_outcome, TaskOutcome::Passed));
}

#[test]
fn test_trace_serde_roundtrip() {
    let mut recorder = InteractionRecorder::new("test-2", "Serde Test", PathBuf::from("/tmp/ss"));

    recorder.record_action(
        WebAction::Click {
            selector: "#btn".into(),
        },
        ActionOutcome::Failed {
            error: "not found".into(),
        },
        50,
        None,
        None,
    );

    let trace = recorder.finish(TaskOutcome::Failed {
        reasons: vec!["element not found".into()],
    });

    let json = serde_json::to_string(&trace).unwrap();
    let parsed: InteractionTrace = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.task_id, "test-2");
    assert_eq!(parsed.actions.len(), 1);
}
