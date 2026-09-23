//! Explicit per-agent workspace root: entering a worktree moves only the
//! agent's root, never the process-global cwd.

use super::*;
use crate::tools::ToolRegistry;
use std::path::Path;

/// A base workspace containing `marker.txt = "base"` and a worktree-like
/// subdirectory `wt/` containing `marker.txt = "worktree"`.
fn base_with_worktree() -> (tempfile::TempDir, PathBuf, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let base = dir.path().canonicalize().unwrap();
    let wt = base.join("wt");
    std::fs::create_dir_all(&wt).unwrap();
    std::fs::write(base.join("marker.txt"), "base").unwrap();
    std::fs::write(wt.join("marker.txt"), "worktree").unwrap();
    (dir, base, wt)
}

fn canon(p: &Path) -> PathBuf {
    p.canonicalize().unwrap()
}

#[test]
fn test_follow_process_cwd_is_not_explicit_and_anchor_is_noop() {
    let root = WorkspaceRoot::follow_process_cwd();
    assert!(!root.is_explicit());
    assert!(!root.is_in_worktree());
    assert!(root.command_dir().is_none());
    // Relative paths stay relative: the OS resolves them against the process
    // cwd, which IS this root — inputs stay byte-identical.
    assert_eq!(root.anchor_str("src/lib.rs"), "src/lib.rs");
    assert_eq!(root.anchor_str("/abs/x"), "/abs/x");
}

#[test]
fn test_enter_does_not_change_process_cwd() {
    // Hold the shared cwd lock only so a concurrent CwdGuard test cannot
    // move the cwd under the before/after comparison.
    let _g = crate::test_support::CwdGuard::hold();
    let (_dir, base, wt) = base_with_worktree();
    let before = std::env::current_dir().unwrap();

    let root = WorkspaceRoot::fixed(&base);
    let entered = root.enter(Path::new("wt")).unwrap();
    assert_eq!(entered, wt);
    assert_eq!(root.path(), wt);
    assert!(root.is_in_worktree());
    assert_eq!(root.current_worktree().as_deref(), Some(wt.as_path()));

    // The process cwd is untouched.
    assert_eq!(std::env::current_dir().unwrap(), before);

    // A follow-cwd root entered into a worktree also leaves the cwd alone.
    let follow = WorkspaceRoot::follow_process_cwd();
    follow.enter(&wt).unwrap();
    assert_eq!(follow.path(), wt);
    assert!(follow.is_explicit());
    assert_eq!(std::env::current_dir().unwrap(), before);
    follow.exit().unwrap();
    assert!(!follow.is_explicit());
}

#[test]
fn test_enter_missing_directory_fails_without_state_change() {
    let (_dir, base, _wt) = base_with_worktree();
    let root = WorkspaceRoot::fixed(&base);
    let err = root.enter(&base.join("does-not-exist")).unwrap_err();
    assert!(err
        .to_string()
        .contains("Failed to change to worktree directory"));
    assert!(!root.is_in_worktree());
    assert_eq!(root.path(), base);
}

#[test]
fn test_exit_restores_root_and_nested_levels() {
    let (_dir, base, wt) = base_with_worktree();
    let inner = wt.join("inner");
    std::fs::create_dir_all(&inner).unwrap();
    let root = WorkspaceRoot::fixed(&base);

    assert!(root
        .exit()
        .unwrap_err()
        .to_string()
        .contains("Not currently in a worktree"));

    root.enter(&wt).unwrap();
    root.enter(Path::new("inner")).unwrap(); // relative to the current level
    assert_eq!(root.path(), inner);

    let (restored, left) = root.exit().unwrap();
    assert_eq!(restored, wt, "nested exit restores the previous level");
    assert_eq!(left, inner);
    assert_eq!(root.path(), wt);

    let (restored, left) = root.exit().unwrap();
    assert_eq!(restored, base);
    assert_eq!(left, wt);
    assert_eq!(root.path(), base);
    assert!(!root.is_in_worktree());
}

#[test]
fn test_exit_fails_when_restore_target_vanished() {
    let (_dir, base, wt) = base_with_worktree();
    let outer = base.join("outer");
    std::fs::create_dir_all(&outer).unwrap();
    let root = WorkspaceRoot::fixed(&base);
    root.enter(&outer).unwrap();
    root.enter(&wt).unwrap();
    std::fs::remove_dir_all(&outer).unwrap();

    let err = root.exit().unwrap_err();
    assert!(
        format!("{err:#}").contains("Failed to restore"),
        "unexpected error: {err:#}"
    );
    // Honest failure: still inside the worktree, nothing popped.
    assert_eq!(root.path(), wt);
}

#[test]
fn test_anchor_json_rewrites_relative_paths_only_when_explicit() {
    let (_dir, base, wt) = base_with_worktree();
    let root = WorkspaceRoot::fixed(&base);
    root.enter(&wt).unwrap();
    let args = serde_json::json!({
        "path": "a.rs",
        "abs": "rel-but-not-a-listed-key",
        "edits": [{"path": "b.rs"}, {"path": "/abs/c.rs"}],
    });
    let out = sync_scope(root.clone(), || {
        anchor_json(args.clone(), &["path", "edits"])
    });
    assert_eq!(out["path"], wt.join("a.rs").to_string_lossy().as_ref());
    assert_eq!(out["abs"], "rel-but-not-a-listed-key");
    assert_eq!(
        out["edits"][0]["path"],
        wt.join("b.rs").to_string_lossy().as_ref()
    );
    assert_eq!(out["edits"][1]["path"], "/abs/c.rs");

    // Not explicit → byte-identical.
    let follow = WorkspaceRoot::follow_process_cwd();
    let out = sync_scope(follow, || anchor_json(args.clone(), &["path", "edits"]));
    assert_eq!(out, args);
}

/// Requirement (2) + (4): after enter, a relative path validates and resolves
/// against the worktree root, shell/cargo subprocesses run there; exit
/// restores the base root. Driven through the registry — the same scoped
/// dispatch the agent uses.
#[cfg(unix)]
#[tokio::test]
async fn test_registry_tools_follow_entered_worktree_and_exit_restores() {
    crate::tools::file::reset_safety_config_for_tests();
    let (_dir, base, wt) = base_with_worktree();
    // A tiny crate only inside the worktree: cargo_check can only succeed
    // when cargo runs in the worktree.
    std::fs::write(
        wt.join("Cargo.toml"),
        "[package]\nname = \"wtprobe\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[workspace]\n",
    )
    .unwrap();
    std::fs::create_dir_all(wt.join("src")).unwrap();
    std::fs::write(wt.join("src/lib.rs"), "pub fn probe() {}\n").unwrap();

    let mut registry = ToolRegistry::new();
    let root = WorkspaceRoot::fixed(&base);
    registry.set_workspace_root(root.clone());

    // Before enter: the relative marker resolves against the base.
    let read = registry
        .execute("file_read", serde_json::json!({"path": "marker.txt"}))
        .await
        .unwrap();
    assert!(read.to_string().contains("base"), "{read}");

    // Validation resolves against the root, not the process cwd.
    sync_scope(root.clone(), || {
        let cfg = crate::config::SafetyConfig::default();
        crate::tools::file::validate_tool_path("marker.txt", &cfg).unwrap();
    });

    root.enter(&wt).unwrap();

    // file_read of the SAME relative path now reads the worktree copy.
    let read = registry
        .execute("file_read", serde_json::json!({"path": "marker.txt"}))
        .await
        .unwrap();
    assert!(read.to_string().contains("worktree"), "{read}");

    // shell_exec runs in the worktree.
    let pwd = registry
        .execute("shell_exec", serde_json::json!({"command": "pwd"}))
        .await
        .unwrap();
    let stdout = pwd["stdout"]
        .as_str()
        .unwrap_or_default()
        .trim()
        .to_string();
    assert_eq!(canon(Path::new(&stdout)), canon(&wt), "{pwd}");

    // cargo runs in the worktree (the only directory with a Cargo.toml).
    let check = registry
        .execute("cargo_check", serde_json::json!({}))
        .await
        .unwrap();
    assert_eq!(
        check["success"], true,
        "cargo_check did not run in the worktree: {check}"
    );

    // Exit restores the base root.
    let (restored, _left) = root.exit().unwrap();
    assert_eq!(restored, base);
    let read = registry
        .execute("file_read", serde_json::json!({"path": "marker.txt"}))
        .await
        .unwrap();
    assert!(read.to_string().contains("base"), "{read}");
    let pwd = registry
        .execute("shell_exec", serde_json::json!({"command": "pwd"}))
        .await
        .unwrap();
    let stdout = pwd["stdout"]
        .as_str()
        .unwrap_or_default()
        .trim()
        .to_string();
    assert_eq!(canon(Path::new(&stdout)), canon(&base), "{pwd}");
}

/// Requirement (3): a concurrent task holding another agent's (original)
/// root keeps resolving against it while this agent enters a worktree.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_concurrent_task_keeps_its_own_root() {
    crate::tools::file::reset_safety_config_for_tests();
    let (_dir, base, wt) = base_with_worktree();
    let agent_a = WorkspaceRoot::fixed(&base);
    let agent_b = WorkspaceRoot::fixed(&base);

    let (tx_started, rx_started) = tokio::sync::oneshot::channel::<()>();
    let (tx_entered, rx_entered) = tokio::sync::oneshot::channel::<()>();

    let observer = tokio::spawn(scope(agent_b.clone(), async move {
        let mut seen = Vec::new();
        let _ = tx_started.send(());
        let mut rx_entered = rx_entered;
        for i in 0..200 {
            seen.push(current_path());
            let cfg = crate::config::SafetyConfig::default();
            crate::tools::file::validate_tool_path("marker.txt", &cfg).unwrap();
            if i == 100 {
                // Make sure part of the loop runs after agent A entered.
                let _ = (&mut rx_entered).await;
            }
            tokio::task::yield_now().await;
        }
        let content = tokio::fs::read_to_string(anchor_path(Path::new("marker.txt")))
            .await
            .unwrap();
        (seen, content)
    }));

    rx_started.await.unwrap();
    scope(agent_a.clone(), async {
        current().enter(&wt).unwrap();
        assert_eq!(current_path(), wt);
    })
    .await;
    let _ = tx_entered.send(());

    let (seen, content) = observer.await.unwrap();
    assert!(seen.iter().all(|p| p == &base), "B observed a foreign root");
    assert_eq!(content, "base");
    assert_eq!(agent_a.path(), wt);
    assert_eq!(agent_b.path(), base);
}

#[test]
fn test_clones_share_one_root() {
    let (_dir, base, wt) = base_with_worktree();
    let root = WorkspaceRoot::fixed(&base);
    let clone = root.clone();
    assert!(root.same_root(&clone));
    clone.enter(&wt).unwrap();
    assert_eq!(root.path(), wt);
    assert!(!root.same_root(&WorkspaceRoot::fixed(&base)));
}

#[cfg(unix)]
#[test]
fn test_command_runs_in_explicit_root() {
    let (_dir, base, wt) = base_with_worktree();
    let root = WorkspaceRoot::fixed(&base);
    root.enter(&wt).unwrap();
    let out = std::process::Command::new("pwd")
        .in_root(&root)
        .output()
        .unwrap();
    let pwd = String::from_utf8_lossy(&out.stdout).trim().to_string();
    assert_eq!(canon(Path::new(&pwd)), canon(&wt));
}
