use super::*;

#[test]
fn test_enter_worktree_tool_name() {
    let tool = EnterWorktreeTool::new();
    assert_eq!(tool.name(), "enter_worktree");
}

#[test]
fn test_exit_worktree_tool_name() {
    let tool = ExitWorktreeTool::new();
    assert_eq!(tool.name(), "exit_worktree");
}

#[test]
fn test_list_worktrees_tool_name() {
    let tool = ListWorktreesTool::new();
    assert_eq!(tool.name(), "list_worktrees");
}

#[test]
fn test_enter_worktree_description() {
    let tool = EnterWorktreeTool::new();
    assert!(tool.description().contains("worktree"));
    assert!(tool.description().contains("isolated"));
}

#[test]
fn test_exit_worktree_description() {
    let tool = ExitWorktreeTool::new();
    assert!(tool.description().contains("Exit"));
    assert!(tool.description().contains("worktree"));
}

#[test]
fn test_list_worktrees_description() {
    let tool = ListWorktreesTool::new();
    assert!(tool.description().contains("List"));
    assert!(tool.description().contains("worktree"));
}

#[test]
fn test_enter_worktree_schema() {
    let tool = EnterWorktreeTool::new();
    let schema = tool.schema();
    assert_eq!(schema["type"], "object");
    assert!(schema["properties"]["path"].is_object());
    assert!(schema["properties"]["branch"].is_object());
}

#[test]
fn test_exit_worktree_schema() {
    let tool = ExitWorktreeTool::new();
    let schema = tool.schema();
    assert_eq!(schema["type"], "object");
    assert!(schema["properties"]["path"].is_object());
    assert!(schema["properties"]["remove"].is_object());
}

#[test]
fn test_list_worktrees_schema() {
    let tool = ListWorktreesTool::new();
    let schema = tool.schema();
    assert_eq!(schema["type"], "object");
}

#[test]
fn test_validate_branch_name_valid() {
    assert!(validate_branch_name("main").is_ok());
    assert!(validate_branch_name("feature-branch").is_ok());
    assert!(validate_branch_name("bugfix/issue-123").is_ok());
    assert!(validate_branch_name("release/v1.0.0").is_ok());
}

#[test]
fn test_validate_branch_name_invalid() {
    assert!(validate_branch_name("").is_err());
    assert!(validate_branch_name("-main").is_err());
    assert!(validate_branch_name("branch;rm -rf /").is_err());
    assert!(validate_branch_name("branch|cat").is_err());
    assert!(validate_branch_name("branch&&evil").is_err());
}

#[test]
fn test_validate_branch_name_long() {
    let long_name = "a".repeat(256);
    assert!(validate_branch_name(&long_name).is_err());
}

// ── validate_path (2026-09-21 review sweep) ──────────────────────────────
//
// The stub used to ignore the safety config and only reject literal null
// bytes; enter_worktree now validates its path with the workspace path
// policy (the path becomes a new directory and the process cwd).

#[test]
fn test_validate_path_allows_workspace_relative_path() {
    // The default config (allowed_paths = ["./**"]) accepts a workspace
    // relative worktree target — the tool's everyday usage.
    let config = SafetyConfig::default();
    assert!(validate_path("feature-branch", Some(&config)).is_ok());
    assert!(validate_path(".selfware/worktrees/x", Some(&config)).is_ok());
}

#[test]
fn test_validate_path_refuses_out_of_workspace_absolute_path() {
    let config = SafetyConfig::default();
    assert!(
        validate_path("/etc/evil", Some(&config)).is_err(),
        "out-of-workspace worktree path must be refused"
    );
}

#[test]
fn test_validate_path_refuses_escape_and_denied_components() {
    let config = SafetyConfig::default();
    // `..` escape chains out of the workspace lexical root.
    assert!(validate_path("../outside", Some(&config)).is_err());
    // Worktrees may not land on a denied path (.env / .ssh shapes).
    assert!(validate_path(".env", Some(&config)).is_err());
}

#[test]
fn test_validate_path_refuses_null_bytes() {
    let config = SafetyConfig::default();
    assert!(validate_path("worktree\0x", Some(&config)).is_err());
}

#[test]
fn test_validate_path_allows_explicit_allowlist() {
    let config = SafetyConfig {
        allowed_paths: vec!["/sandbox/worktrees/**".to_string()],
        ..SafetyConfig::default()
    };
    assert!(validate_path("/sandbox/worktrees/wt-1", Some(&config)).is_ok());
}

// ── enter_worktree_dir path policy (2026-09-21 review finding) ───────────
//
// The TUI `/worktree enter` handler goes through enter_worktree_dir; it used
// to bypass the validation the tool path got in the W1b sweep. enter_worktree_dir
// now runs the same validate_tool_path/resolve_safety_config check with the
// process-global config, resolved against the agent's workspace root, so an
// out-of-workspace target is refused and the root is left untouched. No
// test here touches the process cwd, so none needs the shared cwd lock.

/// A temp workspace pinned as an explicit root (canonical path, so it
/// compares equal to what git reports).
fn temp_root() -> (tempfile::TempDir, PathBuf, WorkspaceRoot) {
    let dir = tempfile::tempdir().unwrap();
    let base = dir.path().canonicalize().unwrap();
    let root = WorkspaceRoot::fixed(&base);
    (dir, base, root)
}

#[test]
fn test_enter_worktree_dir_refuses_out_of_workspace_target() {
    // enter_worktree_dir validates against the process-global config — pin it
    // to the default so an earlier agent-init test cannot leave a permissive
    // one behind.
    crate::tools::file::reset_safety_config_for_tests();
    let (_dir, base, root) = temp_root();
    // The lock only keeps a concurrent CwdGuard test from moving the cwd
    // under the before/after comparison; nothing here changes it.
    let _g = crate::test_support::CwdGuard::hold();
    let cwd_before = std::env::current_dir().unwrap();

    // A tempdir outside the workspace root (default config allows only ./**).
    let outside = tempfile::tempdir().unwrap();
    let err = enter_worktree_dir(&root, outside.path().to_path_buf())
        .expect_err("an out-of-workspace /worktree-enter target must be refused");
    assert!(
        err.to_string().contains("outside the workspace"),
        "unexpected error: {err}"
    );
    // No root change, no state change, no cwd change: the refusal is a pure gate.
    assert_eq!(root.path(), base);
    assert!(!root.is_in_worktree());
    assert!(root.current_worktree().is_none());
    assert_eq!(std::env::current_dir().unwrap(), cwd_before);
}

#[test]
fn test_enter_worktree_dir_allows_in_workspace_target() {
    crate::tools::file::reset_safety_config_for_tests();
    let (_dir, base, root) = temp_root();
    // The lock only keeps a concurrent CwdGuard test from moving the cwd
    // under the before/after comparison; nothing here changes it.
    let _g = crate::test_support::CwdGuard::hold();
    let cwd_before = std::env::current_dir().unwrap();
    let wt = base.join("wt");
    std::fs::create_dir_all(&wt).unwrap();

    let entered = enter_worktree_dir(&root, wt.clone())
        .expect("an in-workspace /worktree-enter target must be allowed");
    assert_eq!(entered, wt);
    assert!(root.is_in_worktree());
    assert_eq!(root.path(), wt);
    // The process cwd never moves.
    assert_eq!(std::env::current_dir().unwrap(), cwd_before);

    let (restored, previous) = exit_worktree_dir(&root).unwrap();
    assert_eq!(restored, base);
    assert_eq!(previous.as_deref(), Some(wt.as_path()));
    assert_eq!(root.path(), base);
    assert!(!root.is_in_worktree());
    assert_eq!(std::env::current_dir().unwrap(), cwd_before);
}

#[test]
fn test_exit_dir_without_enter_errors() {
    let (_dir, base, root) = temp_root();
    let res = exit_worktree_dir(&root);
    assert!(res
        .unwrap_err()
        .to_string()
        .contains("Not currently in a worktree"));
    assert_eq!(root.path(), base);
}

#[test]
fn test_enter_worktree_dir_missing_target_leaves_root_untouched() {
    crate::tools::file::reset_safety_config_for_tests();
    let (_dir, base, root) = temp_root();
    assert!(enter_worktree_dir(&root, base.join("does-not-exist")).is_err());
    assert_eq!(root.path(), base);
    assert!(!root.is_in_worktree());
}

#[test]
fn test_parse_worktree_list_empty() {
    let result = parse_worktree_list("");
    assert!(result.is_empty());
}

#[test]
fn test_parse_worktree_list_single() {
    let output = r#"worktree /path/to/repo
HEAD abc123
branch refs/heads/main
"#;
    let result = parse_worktree_list(output);
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].path, "/path/to/repo");
    assert_eq!(result[0].branch, Some("main".to_string()));
    assert!(!result[0].detached);
}

#[test]
fn test_parse_worktree_list_detached() {
    let output = r#"worktree /path/to/worktree
HEAD def456
detached
"#;
    let result = parse_worktree_list(output);
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].path, "/path/to/worktree");
    assert!(result[0].detached);
    assert!(result[0].branch.is_none());
}

#[test]
fn test_parse_worktree_list_multiple() {
    let output = r#"worktree /path/to/main
HEAD abc123
branch refs/heads/main

worktree /path/to/feature
HEAD def456
branch refs/heads/feature-branch

worktree /path/to/detached
HEAD ghi789
detached
"#;
    let result = parse_worktree_list(output);
    assert_eq!(result.len(), 3);

    assert_eq!(result[0].path, "/path/to/main");
    assert_eq!(result[0].branch, Some("main".to_string()));

    assert_eq!(result[1].path, "/path/to/feature");
    assert_eq!(result[1].branch, Some("feature-branch".to_string()));

    assert_eq!(result[2].path, "/path/to/detached");
    assert!(result[2].detached);
}

#[test]
fn test_enter_worktree_is_not_readonly() {
    let tool = EnterWorktreeTool::new();
    assert!(!tool.is_readonly());
}

#[test]
fn test_exit_worktree_is_not_readonly() {
    let tool = ExitWorktreeTool::new();
    assert!(!tool.is_readonly());
}

#[test]
fn test_list_worktrees_is_readonly() {
    let tool = ListWorktreesTool::new();
    assert!(tool.is_readonly());
}

#[test]
fn test_generate_worktree_name_format() {
    let name = generate_worktree_name();
    assert!(name.starts_with("worktree_"));
    assert!(name.len() > "worktree_".len());
}

#[tokio::test]
async fn test_enter_worktree_validation() {
    let tool = EnterWorktreeTool::new();

    // Test with invalid branch name
    let args = serde_json::json!({
        "branch": "branch; rm -rf /"
    });
    let result = tool.execute(args).await;
    assert!(result.is_err());

    // Test with branch starting with -
    let args = serde_json::json!({
        "branch": "-f"
    });
    let result = tool.execute(args).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_list_worktrees_execute() {
    // Pin the root to the crate directory (a git checkout) instead of relying
    // on the process cwd, so no cwd lock is needed.
    let root = WorkspaceRoot::fixed(env!("CARGO_MANIFEST_DIR"));
    let tool = ListWorktreesTool::new();
    let result = workspace_root::scope(root, tool.execute(serde_json::json!({}))).await;
    assert!(result.is_ok(), "{:?}", result.err());

    let output = result.unwrap();
    assert!(output.get("worktrees").is_some());
    assert!(output.get("count").is_some());
    assert!(output["current_worktree"].is_null());
}

#[tokio::test]
async fn test_exit_worktree_not_in_worktree() {
    let (_dir, _base, root) = temp_root();
    let tool = ExitWorktreeTool::new();
    let result = workspace_root::scope(root, tool.execute(serde_json::json!({}))).await;
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("Not currently in a worktree"));
}

#[test]
fn test_is_in_worktree_follows_the_scoped_root() {
    let (_dir, base, root) = temp_root();
    let wt = base.join("wt");
    std::fs::create_dir_all(&wt).unwrap();

    workspace_root::sync_scope(root.clone(), || {
        assert!(!is_in_worktree());
        assert!(get_current_worktree().is_none());
    });
    root.enter(&wt).unwrap();
    workspace_root::sync_scope(root.clone(), || {
        assert!(is_in_worktree());
        assert_eq!(get_current_worktree().as_deref(), Some(wt.as_path()));
    });
    // Another agent's root is unaffected.
    let (_dir2, _base2, other) = temp_root();
    workspace_root::sync_scope(other, || assert!(!is_in_worktree()));
}

/// Create an isolated git repository WITHOUT changing the process cwd and
/// return it with an explicit workspace root pinned to it.
fn isolated_git_repo() -> (tempfile::TempDir, PathBuf, WorkspaceRoot) {
    let (dir, base, root) = temp_root();
    let git = |args: &[&str]| {
        let status = std::process::Command::new("git")
            .arg("-C")
            .arg(&base)
            .args(args)
            .status()
            .unwrap();
        assert!(status.success(), "git {:?} failed", args);
    };
    git(&["init", "-q"]);
    git(&["config", "user.email", "t@t"]);
    git(&["config", "user.name", "t"]);
    git(&["config", "commit.gpgsign", "false"]);
    std::fs::write(base.join("f.txt"), "x").unwrap();
    git(&["add", "-A"]);
    git(&["commit", "-qm", "base"]);
    (dir, base, root)
}

#[tokio::test]
async fn test_enter_exit_worktree_tool_moves_root_not_cwd() {
    crate::tools::file::reset_safety_config_for_tests();
    let (_dir, base, root) = isolated_git_repo();
    let wt = base.join("wt");

    // A RELATIVE worktree path: validated and resolved against the root.
    let enter = EnterWorktreeTool::with_safety_config(SafetyConfig::default());
    let res = workspace_root::scope(
        root.clone(),
        enter.execute(serde_json::json!({ "path": "wt" })),
    )
    .await;
    let res = res.expect("enter_worktree failed");
    assert_eq!(res["worktree_path"], wt.to_string_lossy().as_ref());
    assert_eq!(res["previous_path"], base.to_string_lossy().as_ref());
    assert!(wt.join("f.txt").is_file(), "git worktree add ran");

    // The agent's root moved (the process cwd is never touched — see
    // test_enter_worktree_dir_allows_in_workspace_target).
    assert_eq!(root.path(), wt);
    assert!(root.is_in_worktree());

    // list_worktrees reports the entered worktree for this root.
    let list = workspace_root::scope(
        root.clone(),
        ListWorktreesTool::new().execute(serde_json::json!({})),
    )
    .await
    .unwrap();
    assert_eq!(list["current_worktree"], wt.to_string_lossy().as_ref());
    assert_eq!(list["count"], 2);

    let exit = ExitWorktreeTool::new();
    let res = workspace_root::scope(
        root.clone(),
        exit.execute(serde_json::json!({ "remove": true })),
    )
    .await
    .expect("exit_worktree failed");
    assert_eq!(res["current_path"], base.to_string_lossy().as_ref());
    assert_eq!(res["removed"], true, "{res}");
    assert!(!wt.exists(), "git worktree remove ran in the restored root");

    assert_eq!(root.path(), base);
    assert!(!root.is_in_worktree());
}

#[tokio::test]
async fn test_enter_worktree_tool_error_preserves_root() {
    crate::tools::file::reset_safety_config_for_tests();
    let (_dir, base, root) = isolated_git_repo();
    let blocker = base.join("blocker");
    std::fs::write(&blocker, "x").unwrap();

    // Worktree path under a regular file: `create_dir_all` fails before any
    // root change, and the failure must leave the root and state untouched.
    let enter = EnterWorktreeTool::with_safety_config(SafetyConfig::default());
    let res = workspace_root::scope(
        root.clone(),
        enter.execute(serde_json::json!({ "path": "blocker/child" })),
    )
    .await;
    assert!(res.is_err());
    assert_eq!(root.path(), base);
    assert!(!root.is_in_worktree());
    assert!(root.current_worktree().is_none());
}

// ---------------------------------------------------------------------------
// Stale worktree pruning
// ---------------------------------------------------------------------------

fn git_in(dir: &std::path::Path, args: &[&str]) -> String {
    let out = std::process::Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .unwrap();
    assert!(out.status.success(), "git {:?} failed: {:?}", args, out);
    String::from_utf8_lossy(&out.stdout).to_string()
}

fn listed_paths(dir: &std::path::Path) -> Vec<String> {
    parse_worktree_list(&git_in(dir, &["worktree", "list", "--porcelain"]))
        .into_iter()
        .map(|e| e.path)
        .collect()
}

#[test]
fn test_parse_worktree_list_marks_prunable() {
    let out = "worktree /repo\nHEAD abc\nbranch refs/heads/main\n\n\
               worktree /repo/.selfware/worktrees/w1\nHEAD abc\ndetached\n\
               prunable gitdir file points to non-existent location\n";
    let entries = parse_worktree_list(out);
    assert!(!entries[0].prunable);
    assert!(entries[1].prunable);
}

#[tokio::test]
async fn test_prune_removes_only_stale_selfware_records_never_directories() {
    let (_dir, base, _root) = isolated_git_repo();
    let stale = base.join(".selfware/worktrees/crashed");
    let live = base.join(".selfware/worktrees/live");
    git_in(
        &base,
        &[
            "worktree",
            "add",
            "-q",
            "--detach",
            &stale.to_string_lossy(),
        ],
    );
    git_in(
        &base,
        &["worktree", "add", "-q", "--detach", &live.to_string_lossy()],
    );
    // Simulate an unexpected termination: the directory vanished, the
    // record lingers.
    std::fs::remove_dir_all(&stale).unwrap();
    assert_eq!(listed_paths(&base).len(), 3);

    let outcome = prune_stale_selfware_worktrees(&base).await.unwrap();
    assert_eq!(outcome, PruneOutcome::Pruned(vec![stale.clone()]));
    let paths = listed_paths(&base);
    assert_eq!(paths.len(), 2, "stale record pruned: {paths:?}");
    assert!(!paths.contains(&stale.to_string_lossy().to_string()));
    assert!(live.join("f.txt").is_file(), "live worktree dir untouched");

    // Idempotent: nothing left to prune.
    assert_eq!(
        prune_stale_selfware_worktrees(&base).await.unwrap(),
        PruneOutcome::NothingStale
    );
}

#[tokio::test]
async fn test_prune_never_touches_worktrees_selfware_did_not_create() {
    let (_dir, base, _root) = isolated_git_repo();
    let users = base.join("users-own-wt");
    let ours = base.join(".selfware/worktrees/crashed");
    git_in(
        &base,
        &[
            "worktree",
            "add",
            "-q",
            "--detach",
            &users.to_string_lossy(),
        ],
    );
    std::fs::remove_dir_all(&users).unwrap();

    // Only a foreign stale record: nothing selfware's, nothing pruned.
    assert_eq!(
        prune_stale_selfware_worktrees(&base).await.unwrap(),
        PruneOutcome::NothingStale
    );
    assert_eq!(listed_paths(&base).len(), 2, "user's record kept");

    // Mixed: `git worktree prune` cannot be scoped, so it is not run.
    git_in(
        &base,
        &["worktree", "add", "-q", "--detach", &ours.to_string_lossy()],
    );
    std::fs::remove_dir_all(&ours).unwrap();
    assert_eq!(
        prune_stale_selfware_worktrees(&base).await.unwrap(),
        PruneOutcome::SkippedForeign {
            selfware: 1,
            foreign: 1
        }
    );
    assert_eq!(listed_paths(&base).len(), 3, "nothing pruned");
}

#[tokio::test]
async fn test_enter_worktree_records_custom_path_and_prunes_it_when_stale() {
    crate::tools::file::reset_safety_config_for_tests();
    let (_dir, base, root) = isolated_git_repo();
    // A custom (non-default-base) path created through the tool is
    // recognised as selfware's via the created-ledger.
    let enter = EnterWorktreeTool::with_safety_config(SafetyConfig::default());
    workspace_root::scope(
        root.clone(),
        enter.execute(serde_json::json!({ "path": "custom_wt" })),
    )
    .await
    .expect("enter_worktree failed");
    let exit = ExitWorktreeTool::new();
    workspace_root::scope(root.clone(), exit.execute(serde_json::json!({})))
        .await
        .expect("exit_worktree failed");

    let custom = base.join("custom_wt");
    std::fs::remove_dir_all(&custom).unwrap();
    assert_eq!(listed_paths(&base).len(), 2);

    // The next enter prunes the stale record before adding a new worktree.
    let res = workspace_root::scope(
        root.clone(),
        enter.execute(serde_json::json!({ "path": "second_wt" })),
    )
    .await
    .expect("second enter_worktree failed");
    assert_eq!(
        res["pruned_stale_worktrees"],
        serde_json::json!([custom.to_string_lossy()])
    );
    let paths = listed_paths(&base);
    assert!(
        !paths.contains(&custom.to_string_lossy().to_string()),
        "{paths:?}"
    );
    assert_eq!(paths.len(), 2, "main + second_wt");
}
