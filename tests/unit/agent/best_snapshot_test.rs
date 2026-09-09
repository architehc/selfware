//! Unit tests for best-snapshot restore (Opus 5 consult: "you submit the last
//! state, not the best state" — snapshot on green verification, restore on
//! failure so an abort doesn't submit a broken end state).

use super::*;

fn write_as_agent(snapshot: &mut AgentSnapshot, path: &Path, content: &str) {
    let paths = [path.to_path_buf()];
    snapshot.before_mutation(&paths).unwrap();
    std::fs::write(path, content).unwrap();
    snapshot.after_mutation(&paths).unwrap();
}

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
    write_as_agent(&mut agent_paths, &deliverable, "# broken rewrite\n");

    agent_paths
        .restore_written(std::slice::from_ref(&deliverable))
        .unwrap();
    agent_paths
        .restore_written(std::slice::from_ref(&deliverable))
        .expect("repeating recovery must remain idempotent");
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
    let mut snap = AgentSnapshot::default();
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

    write_as_agent(&mut snap, &nested, "# nested broken\n");
    write_as_agent(&mut snap, &flat, "# flat broken\n");

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
    write_as_agent(&mut first, &deliverable, "# broken\n");
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
    write_as_agent(&mut snap, &tracked, "# broken\n");
    let created = dir.path().join("extra_output.py");
    write_as_agent(&mut snap, &created, "# created after snapshot\n");
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
    write_as_agent(
        &mut agent.best_snapshot,
        &deliverable,
        "# broken end state\n",
    );
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

/// Regression (review #8): a run that edits ONLY via patch_apply previously
/// produced an empty written set — no snapshot at all, and a green-looking
/// log for a state that was never captured. The full flow must work: green
/// verification snapshots the patched files, failure restores them, and a
/// file created by a post-green patch is rolled back.
#[tokio::test]
async fn patch_apply_only_run_snapshots_and_restores_completely() {
    use crate::agent::Agent;
    use crate::checkpoint::{TaskCheckpoint, ToolCallLog};
    use crate::config::Config;
    use crate::testing::mock_api::MockLlmServer;
    use chrono::Utc;

    let dir = tempfile::tempdir().unwrap();
    let deliverable = dir.path().join("solver.py");
    std::fs::write(&deliverable, "# green state\n").unwrap();

    let server = MockLlmServer::builder().with_response("done").build().await;
    let config = Config {
        endpoint: format!("{}/v1", server.url()),
        ..Default::default()
    };
    let mut agent = Agent::new(config).await.unwrap();

    let log_patch = |cp: &mut TaskCheckpoint, diff: String| {
        cp.log_tool_call(ToolCallLog {
            timestamp: Utc::now(),
            tool_name: "patch_apply".to_string(),
            arguments: serde_json::json!({"diff": diff}).to_string(),
            result: Some("ok".to_string()),
            success: true,
            duration_ms: Some(10),
        });
    };

    let mut cp = TaskCheckpoint::new("t".to_string(), "implement it".to_string());
    log_patch(
        &mut cp,
        format!(
            "--- {0}\n+++ {0}\n@@ -1 +1 @@\n-old\n+# green state\n",
            deliverable.display()
        ),
    );
    agent.current_checkpoint = Some(cp);

    // A passing verification must capture a snapshot even though no
    // file_edit/file_write call ever happened.
    agent.note_green_verification("shell_exec", r#"{"command":"python3 -m pytest"}"#, true);
    assert!(
        agent.best_snapshot.has_snapshot(),
        "green verification must snapshot patch_apply targets"
    );

    // The run degrades: the deliverable is broken and a new file is patched
    // in after the last-green state.
    write_as_agent(
        &mut agent.best_snapshot,
        &deliverable,
        "# broken end state\n",
    );
    let created = dir.path().join("late_helper.py");
    write_as_agent(
        &mut agent.best_snapshot,
        &created,
        "# created after the green state\n",
    );
    let mut cp = agent.current_checkpoint.take().unwrap();
    log_patch(
        &mut cp,
        format!(
            "--- /dev/null\n+++ {0}\n@@ -0,0 +1 @@\n+# created after the green state\n",
            created.display()
        ),
    );
    agent.current_checkpoint = Some(cp);

    // Restore exactly the way task_runner's failure path does.
    let paths = agent.written_paths();
    // written_paths is BTreeSet-ordered: late_helper.py sorts before solver.py.
    assert_eq!(
        paths,
        vec![created.clone(), deliverable.clone()],
        "the written set must cover every patch_apply target"
    );
    agent.best_snapshot.restore_written(&paths).unwrap();
    assert_eq!(
        std::fs::read_to_string(&deliverable).unwrap(),
        "# green state\n",
        "the patched deliverable must roll back to the last-green bytes"
    );
    assert!(
        !created.exists(),
        "a file created by a post-green patch must not survive restore"
    );
    server.stop().await;
}

/// Regression (review #8): file_multi_edit carries a LIST of per-edit paths —
/// every targeted file must land in the written set so capture and restore
/// cover the whole batch.
#[tokio::test]
async fn multi_edit_run_covers_every_target_file() {
    use crate::agent::Agent;
    use crate::checkpoint::{TaskCheckpoint, ToolCallLog};
    use crate::config::Config;
    use crate::testing::mock_api::MockLlmServer;
    use chrono::Utc;

    let dir = tempfile::tempdir().unwrap();
    let files: Vec<_> = ["one.py", "two.py", "sub/three.py"]
        .iter()
        .map(|name| dir.path().join(name))
        .collect();
    std::fs::create_dir_all(dir.path().join("sub")).unwrap();
    for f in &files {
        std::fs::write(f, "# green state\n").unwrap();
    }

    let server = MockLlmServer::builder().with_response("done").build().await;
    let config = Config {
        endpoint: format!("{}/v1", server.url()),
        ..Default::default()
    };
    let mut agent = Agent::new(config).await.unwrap();

    let edits: Vec<_> = files
        .iter()
        .map(|f| {
            serde_json::json!({
                "path": f.to_string_lossy(),
                "old_str": "old",
                "new_str": "new"
            })
        })
        .collect();
    let mut cp = TaskCheckpoint::new("t".to_string(), "implement it".to_string());
    cp.log_tool_call(ToolCallLog {
        timestamp: Utc::now(),
        tool_name: "file_multi_edit".to_string(),
        arguments: serde_json::json!({"edits": edits}).to_string(),
        result: Some("ok".to_string()),
        success: true,
        duration_ms: Some(10),
    });
    agent.current_checkpoint = Some(cp);

    agent.note_green_verification("shell_exec", r#"{"command":"python3 -m pytest"}"#, true);
    assert!(
        agent.best_snapshot.has_snapshot(),
        "green verification must snapshot file_multi_edit targets"
    );

    let paths = agent.written_paths();
    let mut expected = files.clone();
    expected.sort(); // written_paths is BTreeSet-ordered
    assert_eq!(
        paths, expected,
        "every file_multi_edit target must be in the written set"
    );

    for f in &files {
        write_as_agent(&mut agent.best_snapshot, f, "# broken end state\n");
    }
    agent.best_snapshot.restore_written(&paths).unwrap();
    for f in &files {
        assert_eq!(
            std::fs::read_to_string(f).unwrap(),
            "# green state\n",
            "{} must roll back to the last-green bytes",
            f.display()
        );
    }
    server.stop().await;
}

#[test]
fn existing_file_first_touched_after_green_is_restored_not_deleted() {
    let dir = tempfile::tempdir().unwrap();
    let first = dir.path().join("first.py");
    let later = dir.path().join("later.py");
    std::fs::write(&first, "verified").unwrap();
    std::fs::write(&later, "preexisting user contents").unwrap();
    let mut snapshot = AgentSnapshot::default();
    snapshot
        .snapshot_written(std::slice::from_ref(&first))
        .unwrap();
    write_as_agent(&mut snapshot, &later, "broken later edit");
    snapshot.restore_written(&[first, later.clone()]).unwrap();
    assert_eq!(
        std::fs::read_to_string(later).unwrap(),
        "preexisting user contents"
    );
}

#[test]
fn unknown_failed_call_target_is_never_removed() {
    let dir = tempfile::tempdir().unwrap();
    let first = dir.path().join("first.py");
    let denied = dir.path().join("denied.py");
    std::fs::write(&first, "verified").unwrap();
    std::fs::write(&denied, "never edited").unwrap();
    let mut snapshot = AgentSnapshot::default();
    snapshot
        .snapshot_written(std::slice::from_ref(&first))
        .unwrap();
    // Validation rejected this call before the pre-mutation hook ran.
    snapshot.restore_written(&[first, denied.clone()]).unwrap();
    assert_eq!(std::fs::read_to_string(denied).unwrap(), "never edited");
}

#[test]
fn relative_and_absolute_aliases_restore_once() {
    let dir = tempfile::tempdir().unwrap();
    let _cwd = crate::test_support::CwdGuard::enter(dir.path());
    let relative = PathBuf::from("solver.py");
    let absolute = dir.path().join("solver.py");
    std::fs::write(&relative, "verified").unwrap();
    let mut snapshot = AgentSnapshot::default();
    snapshot
        .snapshot_written(std::slice::from_ref(&relative))
        .unwrap();
    write_as_agent(&mut snapshot, &absolute, "broken");
    snapshot
        .restore_written(&[relative, absolute.clone()])
        .unwrap();
    assert_eq!(std::fs::read_to_string(absolute).unwrap(), "verified");
}

#[test]
fn failed_partial_write_and_deletion_restore_preimages() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("solver.py");
    std::fs::write(&file, "verified").unwrap();
    let paths = [file.clone()];
    let mut snapshot = AgentSnapshot::default();
    snapshot.snapshot_written(&paths).unwrap();
    snapshot.before_mutation(&paths).unwrap();
    std::fs::remove_file(&file).unwrap();
    // The post-hook runs regardless of tool success.
    snapshot.after_mutation(&paths).unwrap();
    snapshot.restore_written(&paths).unwrap();
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "verified");

    snapshot.before_mutation(&paths).unwrap();
    std::fs::remove_file(&file).unwrap();
    snapshot.after_mutation(&paths).unwrap();
    snapshot.snapshot_written(&paths).unwrap();
    write_as_agent(&mut snapshot, &file, "recreated after verified deletion");
    snapshot.restore_written(&paths).unwrap();
    assert!(!file.exists(), "verified absence must be restored");
}

#[test]
fn recovery_preserves_external_edits_and_uncaptured_targets() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("solver.py");
    std::fs::write(&file, "verified").unwrap();
    let paths = [file.clone()];
    let mut snapshot = AgentSnapshot::default();
    snapshot.snapshot_written(&paths).unwrap();
    write_as_agent(&mut snapshot, &file, "agent version");
    std::fs::write(&file, "subsequent user edit").unwrap();
    assert!(snapshot.restore_written(&paths).is_err());
    assert_eq!(
        std::fs::read_to_string(file).unwrap(),
        "subsequent user edit"
    );
}

#[test]
fn external_edit_blocks_later_agent_write_before_preimage_is_lost() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("existing.py");
    std::fs::write(&file, "verified original").unwrap();
    let mut snapshot = AgentSnapshot::default();
    snapshot
        .snapshot_written(std::slice::from_ref(&file))
        .unwrap();
    write_as_agent(&mut snapshot, &file, "agent revision");
    std::fs::write(&file, "concurrent user revision").unwrap();
    let error = snapshot
        .before_mutation(std::slice::from_ref(&file))
        .expect_err("a later agent write must not erase unobserved user work");
    assert!(error.to_string().contains("externally changed"));
    assert_eq!(
        std::fs::read_to_string(&file).unwrap(),
        "concurrent user revision"
    );
    assert!(snapshot
        .restore_written(std::slice::from_ref(&file))
        .is_err());
    assert_eq!(
        std::fs::read_to_string(&file).unwrap(),
        "concurrent user revision"
    );
}

#[test]
fn multi_edit_aliases_share_one_preflight_observation() {
    let dir = tempfile::tempdir().unwrap();
    let _cwd = crate::test_support::CwdGuard::enter(dir.path());
    let file = dir.path().join("existing.py");
    std::fs::write(&file, "original").unwrap();
    let mut snapshot = AgentSnapshot::default();
    snapshot
        .snapshot_written(std::slice::from_ref(&file))
        .unwrap();
    let paths = [file.clone(), PathBuf::from("./existing.py")];
    snapshot.before_mutation(&paths).unwrap();
    std::fs::write(&file, "edited twice").unwrap();
    snapshot.after_mutation(&paths).unwrap();
    snapshot.restore_written(&paths).unwrap();
    assert_eq!(std::fs::read_to_string(file).unwrap(), "original");
}

#[test]
fn failed_post_observation_does_not_discard_other_file_recovery() {
    let dir = tempfile::tempdir().unwrap();
    let first = dir.path().join("first.py");
    let second = dir.path().join("second.py");
    std::fs::write(&first, "first green").unwrap();
    std::fs::write(&second, "second green").unwrap();
    let paths = [first.clone(), second.clone()];
    let mut snapshot = AgentSnapshot::default();
    snapshot.snapshot_written(&paths).unwrap();
    snapshot.before_mutation(&paths).unwrap();
    // A failed shell call can both change a target's type and damage another.
    std::fs::remove_file(&first).unwrap();
    std::fs::create_dir(&first).unwrap();
    std::fs::write(&second, "second broken").unwrap();
    assert!(snapshot.after_mutation(&paths).is_err());
    assert!(snapshot.restore_written(&paths).is_err());
    assert!(
        first.is_dir(),
        "an unexpected directory must not be deleted"
    );
    assert_eq!(std::fs::read_to_string(second).unwrap(), "second green");
}

#[tokio::test]
async fn canonical_snapshot_paths_survive_historical_relative_arguments() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("solver.py");
    std::fs::write(&file, "verified").unwrap();
    let mut agent =
        crate::agent::Agent::new(crate::test_support::mock_agent_config("http://127.0.0.1:1"))
            .await
            .unwrap();
    let mut checkpoint =
        crate::checkpoint::TaskCheckpoint::new("identity".into(), "Fix solver.py".into());
    // Simulate a log written in a previous worktree. Resolving this argument
    // in the current cwd no longer identifies the captured file.
    checkpoint.log_tool_call(crate::checkpoint::ToolCallLog {
        timestamp: chrono::Utc::now(),
        tool_name: "file_write".into(),
        arguments: serde_json::json!({"path": "old-worktree/solver.py", "content": "verified"})
            .to_string(),
        result: Some("ok".into()),
        success: true,
        duration_ms: Some(1),
    });
    agent.current_checkpoint = Some(checkpoint);
    agent
        .best_snapshot
        .snapshot_written(std::slice::from_ref(&file))
        .unwrap();
    write_as_agent(&mut agent.best_snapshot, &file, "broken");
    let paths = agent.written_paths();
    assert!(paths.contains(&std::fs::canonicalize(&file).unwrap()));
    agent.best_snapshot.restore_written(&paths).unwrap();
    assert_eq!(std::fs::read_to_string(file).unwrap(), "verified");
}
