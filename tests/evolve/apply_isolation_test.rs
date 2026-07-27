//! Shadow-worktree isolation for apply runs (Task 1 of apply-isolation):
//! staging a run must create a shadow worktree at the live HEAD, run records
//! must carry `shadow_path` + `base_revision`, writes in the shadow must not
//! touch the live checkout, and cleanup must remove the shadow.
//!
//! Bounded diff verification (Task 2): an in-scope change must produce a
//! populated `StagedDiff` with a stable digest; a change outside `src/` +
//! `docs/` must be rejected with the offending path named; no change at all
//! must be rejected as `empty_diff`.
//!
//! One-use commit endpoint (Task 3): POST /api/actions/apply/commit merges a
//! Staged run into the live checkout only with the exact diff digest and an
//! unmoved live HEAD; a merged run is consumed (second call → 404).

use std::path::Path;

use axum::http::StatusCode;
use git2::{Repository, Signature};
use serde_json::{json, Value};

use selfware::evolution::ast_tools::cleanup_worktree;
use selfware::evolve::apply::{self, ApplyError, ApplyStatus, RejectReason};
use selfware::evolve::server::EvolveServer;
use selfware::evolve::Graph;

use crate::post_json;

/// Temp repo with one committed file; returns (dir, HEAD oid).
fn committed_repository() -> (tempfile::TempDir, String) {
    let project = tempfile::tempdir().unwrap();
    let repo = Repository::init(project.path()).unwrap();
    std::fs::write(project.path().join("README.md"), "initial\n").unwrap();

    let mut index = repo.index().unwrap();
    index.add_path(std::path::Path::new("README.md")).unwrap();
    index.write().unwrap();
    let tree_id = index.write_tree().unwrap();
    let tree = repo.find_tree(tree_id).unwrap();
    let signature = Signature::now("Selfware Test", "selfware@example.test").unwrap();
    let head = repo
        .commit(
            Some("HEAD"),
            &signature,
            &signature,
            "initial commit",
            &tree,
            &[],
        )
        .unwrap()
        .to_string();
    (project, head)
}

#[tokio::test]
async fn staged_run_writes_isolated_from_live_checkout() {
    let (project, head) = committed_repository();
    let registry = apply::new_registry();

    let staged = apply::stage_run(
        "append a line to README".to_string(),
        project.path().to_path_buf(),
        registry.clone(),
    )
    .await
    .expect("staging a run in a clean repo must succeed");

    // Run registered as Running with shadow + base revision recorded.
    let run = apply::get(&registry, &staged.id)
        .await
        .expect("staged run is registered");
    assert_eq!(run.status, ApplyStatus::Running);
    assert_eq!(
        run.shadow_path.as_deref(),
        Some(staged.shadow_path.as_path())
    );
    assert_eq!(run.base_revision.as_deref(), Some(head.as_str()));
    assert_eq!(staged.base_revision, head);
    assert!(run.diff.is_none());

    // Shadow exists at HEAD: it contains the committed file content.
    assert!(staged.shadow_path.exists());
    let shadow_file = staged.shadow_path.join("README.md");
    assert_eq!(std::fs::read_to_string(&shadow_file).unwrap(), "initial\n");

    // A write made in the shadow (what the agent would do) does NOT appear in
    // the live checkout while the run is staged/running.
    std::fs::write(&shadow_file, "initial\nagent change\n").unwrap();
    std::fs::write(staged.shadow_path.join("NEW_FILE.md"), "created by agent\n").unwrap();
    assert_eq!(
        std::fs::read_to_string(project.path().join("README.md")).unwrap(),
        "initial\n"
    );
    assert!(!project.path().join("NEW_FILE.md").exists());

    // Cleanup removes the shadow; the live checkout stays byte-identical.
    cleanup_worktree(project.path(), &staged.shadow_path).unwrap();
    assert!(!staged.shadow_path.exists());
    assert_eq!(
        std::fs::read_to_string(project.path().join("README.md")).unwrap(),
        "initial\n"
    );
    drop(staged.guard);
}

#[tokio::test]
async fn staging_failure_marks_run_failed_with_typed_error() {
    // Not a git repo: staging must fail. HEAD is read BEFORE the shadow is
    // created (base-pinning correctness), so the typed error is BaseRevision.
    let dir = tempfile::tempdir().unwrap();
    let registry = apply::new_registry();

    let err = apply::stage_run(
        "do something".to_string(),
        dir.path().to_path_buf(),
        registry.clone(),
    )
    .await
    .expect_err("staging outside a git repo must fail");
    assert!(matches!(err, ApplyError::BaseRevision(_)));

    // The failed attempt is registered honestly as Failed (AGENTS.md §3).
    let runs = registry.lock().await;
    assert_eq!(runs.len(), 1);
    let run = runs.values().next().unwrap();
    assert_eq!(run.status, ApplyStatus::Failed);
    assert!(run.shadow_path.is_none());
    assert!(run.base_revision.is_none());
}

#[tokio::test]
async fn in_scope_change_produces_staged_diff_with_stable_digest() {
    let (project, _head) = committed_repository();
    let registry = apply::new_registry();
    let staged = apply::stage_run(
        "add src and docs changes".to_string(),
        project.path().to_path_buf(),
        registry.clone(),
    )
    .await
    .expect("staging must succeed");

    // What the agent would write: new files inside src/ and docs/.
    std::fs::create_dir_all(staged.shadow_path.join("src")).unwrap();
    std::fs::write(
        staged.shadow_path.join("src/main.rs"),
        "fn main() { println!(\"hi\"); }\n",
    )
    .unwrap();
    std::fs::create_dir_all(staged.shadow_path.join("docs")).unwrap();
    std::fs::write(staged.shadow_path.join("docs/notes.md"), "# notes\n").unwrap();

    let first = apply::verify_staged_diff(&staged.shadow_path, &staged.base_revision)
        .expect("git2 diff computation must not fail")
        .expect("in-scope diff must be accepted");
    assert_eq!(first.files_changed, 2);
    assert_eq!(first.insertions, 2);
    assert_eq!(first.deletions, 0);
    assert!(first.preview.contains("src/main.rs"));
    assert!(first.preview.contains("docs/notes.md"));
    assert!(first.preview.len() <= 8 * 1024);
    assert_eq!(first.digest.len(), 64, "sha256 hex digest");

    // The digest binds the exact patch text: recomputing must be identical.
    let second = apply::verify_staged_diff(&staged.shadow_path, &staged.base_revision)
        .expect("git2 diff computation must not fail")
        .expect("in-scope diff must be accepted");
    assert_eq!(first.digest, second.digest);
    assert_eq!(first.preview, second.preview);

    cleanup_worktree(project.path(), &staged.shadow_path).unwrap();
    drop(staged.guard);
}

#[tokio::test]
async fn out_of_scope_change_is_rejected_with_path_named() {
    let (project, _head) = committed_repository();
    let registry = apply::new_registry();
    let staged = apply::stage_run(
        "touch build.rs".to_string(),
        project.path().to_path_buf(),
        registry.clone(),
    )
    .await
    .expect("staging must succeed");

    // A change at the repo root is outside src/ + docs/.
    std::fs::write(staged.shadow_path.join("build.rs"), "fn main() {}\n").unwrap();

    let rejection = apply::verify_staged_diff(&staged.shadow_path, &staged.base_revision)
        .expect("git2 diff computation must not fail")
        .expect_err("out-of-scope diff must be rejected");
    assert_eq!(rejection, RejectReason::OutOfScope("build.rs".to_string()));
    assert_eq!(rejection.to_string(), "diff_out_of_scope: build.rs");

    cleanup_worktree(project.path(), &staged.shadow_path).unwrap();
    drop(staged.guard);
}

#[tokio::test]
async fn no_change_is_rejected_as_empty_diff() {
    let (project, _head) = committed_repository();
    let registry = apply::new_registry();
    let staged = apply::stage_run(
        "do nothing".to_string(),
        project.path().to_path_buf(),
        registry.clone(),
    )
    .await
    .expect("staging must succeed");

    let rejection = apply::verify_staged_diff(&staged.shadow_path, &staged.base_revision)
        .expect("git2 diff computation must not fail")
        .expect_err("an empty diff must be rejected");
    assert_eq!(rejection, RejectReason::Empty);
    assert_eq!(rejection.to_string(), "empty_diff");

    cleanup_worktree(project.path(), &staged.shadow_path).unwrap();
    drop(staged.guard);
}

// --- Task 3: one-use commit endpoint -------------------------------------

/// Stage a run in the server's registry, simulate the agent writing an
/// in-scope `src/agent.rs`, verify the diff, and mark the run Staged (what the
/// exit-watcher does on a clean agent exit). Returns (run_id, diff_digest,
/// shadow_path). The staging lock guard is dropped, as it would be when the
/// agent process exits, so the commit step can take APPLY_LOCK.
async fn staged_agent_run(
    server: &EvolveServer,
    root: &Path,
) -> (String, String, std::path::PathBuf) {
    let registry = server.apply_registry();
    let staged = apply::stage_run(
        "add src/agent.rs".to_string(),
        root.to_path_buf(),
        registry.clone(),
    )
    .await
    .expect("staging must succeed");

    std::fs::create_dir_all(staged.shadow_path.join("src")).unwrap();
    std::fs::write(
        staged.shadow_path.join("src/agent.rs"),
        "pub fn agent() {}\n",
    )
    .unwrap();

    let diff = apply::verify_staged_diff(&staged.shadow_path, &staged.base_revision)
        .expect("git2 diff computation must not fail")
        .expect("in-scope diff must be accepted");
    let run_id = staged.id.clone();
    let digest = diff.digest.clone();
    let shadow_path = staged.shadow_path.clone();
    if let Some(run) = registry.lock().await.get_mut(&run_id) {
        run.status = ApplyStatus::Staged;
        run.diff = Some(diff);
    }
    drop(staged.guard);
    (run_id, digest, shadow_path)
}

/// Commit the given paths in the repo, advancing HEAD; returns the new oid.
/// (Adds paths explicitly rather than `add_all`, which refuses the nested
/// shadow worktree under `.worktrees/`.)
fn commit_paths(repo: &Repository, paths: &[&str], message: &str) -> String {
    let mut index = repo.index().unwrap();
    for path in paths {
        index.add_path(Path::new(path)).unwrap();
    }
    index.write().unwrap();
    let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
    let signature = Signature::now("Selfware Test", "selfware@example.test").unwrap();
    let parent = repo.head().unwrap().peel_to_commit().unwrap();
    repo.commit(
        Some("HEAD"),
        &signature,
        &signature,
        message,
        &tree,
        &[&parent],
    )
    .unwrap()
    .to_string()
}

/// Server + staged run over a temp repo; returns the pieces each commit test
/// needs. The project path is canonicalized so git worktree admin paths match
/// the server's canonical project root (macOS /var → /private/var symlink).
async fn staged_fixture() -> (
    tempfile::TempDir,
    std::path::PathBuf,
    EvolveServer,
    String,
    String,
    String,
    std::path::PathBuf,
) {
    let (project, base_head) = committed_repository();
    let root = std::fs::canonicalize(project.path()).unwrap();
    let server = EvolveServer::for_project(Graph::default(), &root).unwrap();
    let (run_id, digest, shadow_path) = staged_agent_run(&server, &root).await;
    (
        project,
        root,
        server,
        base_head,
        run_id,
        digest,
        shadow_path,
    )
}

/// (a) Commit with the correct digest merges into the live checkout.
#[tokio::test]
async fn commit_with_correct_digest_merges_into_live_checkout() {
    let (_project, root, server, base_head, run_id, digest, shadow_path) = staged_fixture().await;

    let (status, body) = post_json(
        &server,
        "/api/actions/apply/commit",
        json!({ "run_id": run_id, "diff_digest": digest }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let body: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(body["merged"], true);
    assert_eq!(body["files_changed"], 1);
    let new_head = body["new_head"].as_str().expect("new_head is a string");
    assert_ne!(new_head, base_head, "HEAD must advance");

    // The live checkout now contains the staged change.
    assert_eq!(
        std::fs::read_to_string(root.join("src/agent.rs")).unwrap(),
        "pub fn agent() {}\n"
    );

    // HEAD points at a merge commit that references the run id.
    let repo = Repository::open(&root).unwrap();
    let head = repo.head().unwrap().peel_to_commit().unwrap();
    assert_eq!(head.id().to_string(), new_head);
    assert_eq!(head.parent_id(0).unwrap().to_string(), base_head);
    assert!(
        head.message().unwrap_or_default().contains(&run_id),
        "merge commit message references the run id: {:?}",
        head.message()
    );

    // One-use: the run is consumed and the shadow worktree is gone.
    assert!(apply::get(&server.apply_registry(), &run_id)
        .await
        .is_none());
    assert!(!shadow_path.exists(), "shadow worktree is cleaned up");
}

/// (b) A second commit call with the same token → 404 unknown_run.
#[tokio::test]
async fn second_commit_call_is_consumed_unknown_run() {
    let (_project, _root, server, _base, run_id, digest, _shadow) = staged_fixture().await;

    let request = json!({ "run_id": run_id, "diff_digest": digest });
    let (status, body) = post_json(&server, "/api/actions/apply/commit", request.clone()).await;
    assert_eq!(status, StatusCode::OK, "{body}");

    let (status, body) = post_json(&server, "/api/actions/apply/commit", request).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    assert!(body.contains("unknown_run"), "{body}");
}

/// (c) A wrong digest → 404 unknown_run, and the run is NOT consumed.
#[tokio::test]
async fn commit_with_wrong_digest_is_unknown_run() {
    let (_project, _root, server, _base, run_id, _digest, _shadow) = staged_fixture().await;

    let (status, body) = post_json(
        &server,
        "/api/actions/apply/commit",
        json!({ "run_id": run_id, "diff_digest": "0".repeat(64) }),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    assert!(body.contains("unknown_run"), "{body}");

    // A bad token must not consume the run: the correct digest still works.
    let run = apply::get(&server.apply_registry(), &run_id)
        .await
        .expect("a wrong digest must not consume the run");
    assert_eq!(run.status, ApplyStatus::Staged);
}

/// (d) Live HEAD moved between staging and commit → 409 base_moved.
#[tokio::test]
async fn commit_after_live_head_moved_is_base_moved_conflict() {
    let (_project, root, server, _base, run_id, digest, _shadow) = staged_fixture().await;

    // Advance the live checkout's HEAD after staging (someone else's commit).
    std::fs::write(root.join("README.md"), "initial\nmoved\n").unwrap();
    let repo = Repository::open(&root).unwrap();
    commit_paths(&repo, &["README.md"], "unrelated commit moving HEAD");

    let (status, body) = post_json(
        &server,
        "/api/actions/apply/commit",
        json!({ "run_id": run_id, "diff_digest": digest }),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert!(body.contains("base_moved"), "{body}");

    // The conflict did not merge anything and did not consume the run.
    assert!(!root.join("src/agent.rs").exists());
    let run = apply::get(&server.apply_registry(), &run_id)
        .await
        .expect("a base_moved conflict must not consume the run");
    assert_eq!(run.status, ApplyStatus::Staged);
}
