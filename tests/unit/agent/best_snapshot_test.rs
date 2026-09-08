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

// --- Regression: reviewer probes against the pre-fix implementation. ---

/// Probe 1: flattened slot names collided (`a/b.py` vs `a_b.py`), so restore
/// wrote one file's bytes into the other. Mirrored paths must stay distinct.
#[test]
fn distinct_paths_never_collide() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("a")).unwrap();
    let nested = dir.path().join("a").join("b.py");
    let flat = dir.path().join("a_b.py");
    std::fs::write(&nested, "# nested green\n").unwrap();
    std::fs::write(&flat, "# flat green\n").unwrap();

    let mut snap = AgentSnapshot::default();
    let paths = vec![nested.clone(), flat.clone()];
    snap.snapshot_written(&paths).unwrap();

    std::fs::write(&nested, "# nested broken\n").unwrap();
    std::fs::write(&flat, "# flat broken\n").unwrap();

    snap.restore_written(&paths).unwrap();
    assert_eq!(
        std::fs::read_to_string(&nested).unwrap(),
        "# nested green\n"
    );
    assert_eq!(std::fs::read_to_string(&flat).unwrap(), "# flat green\n");
}

/// Probe 2: both instances shared one PID-keyed dir, so one instance's
/// `clear()` deleted the other's captured state. Directories are now
/// instance-unique.
#[test]
fn clear_only_affects_the_owning_instance() {
    let dir = tempfile::tempdir().unwrap();
    let deliverable = dir.path().join("solver.py");
    std::fs::write(&deliverable, "# green\n").unwrap();

    let mut first = AgentSnapshot::default();
    first
        .snapshot_written(std::slice::from_ref(&deliverable))
        .unwrap();

    // A second worker in the same process starts a fresh task.
    let mut second = AgentSnapshot::default();
    second.clear();

    assert!(
        first.has_snapshot(),
        "another instance's clear() must not drop this snapshot"
    );
    std::fs::write(&deliverable, "# broken\n").unwrap();
    first
        .restore_written(std::slice::from_ref(&deliverable))
        .unwrap();
    assert_eq!(
        std::fs::read_to_string(&deliverable).unwrap(),
        "# green\n",
        "the first instance's captured bytes must survive another instance's clear()"
    );
}

/// Probe 3: a file created after the snapshot survived `restore()`. Restore
/// must remove post-capture creations inside the written set while leaving
/// unrelated files alone.
#[test]
fn restore_removes_files_created_after_the_snapshot() {
    let dir = tempfile::tempdir().unwrap();
    let tracked = dir.path().join("solver.py");
    std::fs::write(&tracked, "# green\n").unwrap();

    let mut snap = AgentSnapshot::default();
    snap.snapshot_written(std::slice::from_ref(&tracked))
        .unwrap();

    // The agent keeps working past the green state: edits the tracked file,
    // writes a new deliverable, and an unrelated file exists nearby.
    std::fs::write(&tracked, "# broken\n").unwrap();
    let created = dir.path().join("extra_output.py");
    std::fs::write(&created, "# created after snapshot\n").unwrap();
    let unrelated = dir.path().join("unrelated_notes.txt");
    std::fs::write(&unrelated, "user work\n").unwrap();

    // The caller's written set covers both agent-written files; the
    // unrelated file is never passed in.
    snap.restore_written(&[tracked.clone(), created.clone()])
        .unwrap();

    assert_eq!(
        std::fs::read_to_string(&tracked).unwrap(),
        "# green\n",
        "tracked file must roll back to the last-green bytes"
    );
    assert!(
        !created.exists(),
        "a file created after the snapshot must not survive restore"
    );
    assert_eq!(
        std::fs::read_to_string(&unrelated).unwrap(),
        "user work\n",
        "files outside the written set must be left alone"
    );
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
