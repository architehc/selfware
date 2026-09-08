//! Unit tests for best-snapshot restore (Opus 5 consult: "you submit the last
//! state, not the best state" — snapshot on green verification, restore on
//! failure so an abort doesn't submit a broken end state).

use super::*;

#[test]
fn snapshot_copies_written_files_and_restore_brings_them_back() {
    let dir = tempfile::tempdir().unwrap();
    let deliverable = dir.path().join("solver.py");
    std::fs::write(&deliverable, "# green version\n").unwrap();

    let mut agent_paths = AgentSnapshot::default();
    agent_paths
        .snapshot_written(std::slice::from_ref(&deliverable))
        .unwrap();

    // The agent keeps editing after the green run — broken end state.
    std::fs::write(&deliverable, "# broken rewrite\n").unwrap();

    agent_paths
        .restore_written(std::slice::from_ref(&deliverable))
        .unwrap();
    assert_eq!(
        std::fs::read_to_string(&deliverable).unwrap(),
        "# green version\n",
        "restore must bring back the last-green content"
    );
}

#[test]
fn restore_without_snapshot_is_a_noop() {
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("nothing.py");
    let snap = AgentSnapshot::default();
    // No snapshot ever taken: must not panic, must not create the file.
    snap.restore_written(std::slice::from_ref(&target)).unwrap();
    assert!(!target.exists());
}

#[test]
fn snapshot_skips_missing_files() {
    let dir = tempfile::tempdir().unwrap();
    let missing = dir.path().join("deleted.py");
    let mut snap = AgentSnapshot::default();
    // A path that vanished between write and snapshot must not error.
    snap.snapshot_written(std::slice::from_ref(&missing))
        .unwrap();
}

// --- Agent-level capture hook: a green verification snapshots the written
// deliverables so a later failure restores them (submit best, not last). ---

#[tokio::test]
async fn green_verification_snapshots_and_failure_restores() {
    use crate::agent::Agent;
    use crate::checkpoint::{TaskCheckpoint, ToolCallLog};
    use crate::config::Config;
    use crate::testing::mock_api::MockLlmServer;
    use chrono::Utc;

    let dir = tempfile::tempdir().unwrap();
    let deliverable = dir.path().join("deliverable.py");
    std::fs::write(&deliverable, "# green state\n").unwrap();

    let server = MockLlmServer::builder().with_response("done").build().await;
    let config = Config {
        endpoint: format!("{}/v1", server.url()),
        ..Default::default()
    };
    let mut agent = Agent::new(config).await.unwrap();

    let mut cp = TaskCheckpoint::new("t".to_string(), "implement it".to_string());
    cp.log_tool_call(ToolCallLog {
        timestamp: Utc::now(),
        tool_name: "file_write".to_string(),
        arguments: serde_json::json!({"path": deliverable.to_string_lossy(), "content": "x"})
            .to_string(),
        result: Some("ok".to_string()),
        success: true,
        duration_ms: Some(10),
    });
    agent.current_checkpoint = Some(cp);

    // A passing verification captures the snapshot.
    agent.note_green_verification("shell_exec", r#"{"command":"python3 -m pytest"}"#, true);
    assert!(
        agent.best_snapshot.has_snapshot(),
        "green verification must snapshot the written files"
    );

    // The agent breaks the deliverable afterwards, then the run would fail.
    std::fs::write(&deliverable, "# broken end state\n").unwrap();
    agent
        .best_snapshot
        .restore_written(std::slice::from_ref(&deliverable))
        .unwrap();
    assert_eq!(
        std::fs::read_to_string(&deliverable).unwrap(),
        "# green state\n",
        "the last-green content must come back"
    );
    server.stop().await;
}
