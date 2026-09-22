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
// process-global config, so an out-of-workspace target is refused and the cwd
// is left untouched.

#[test]
fn test_enter_worktree_dir_refuses_out_of_workspace_target() {
    // Hold the shared cwd lock so the validate (resolved against the process
    // cwd and the global safety config) sees a stable working directory.
    let _g = crate::test_support::CwdGuard::hold();
    // enter_worktree_dir validates against the process-global config — pin it
    // to the default so an earlier agent-init test cannot leave a permissive
    // one behind (which would let /var/folders paths through).
    crate::tools::file::reset_safety_config_for_tests();
    reset_worktree_state();
    let before = std::env::current_dir().unwrap();

    // A tempdir outside the workspace (default config allows only ./**).
    let outside = tempfile::tempdir().unwrap();
    let err = crate::tools::git_worktree::enter_worktree_dir(outside.path().to_path_buf())
        .expect_err("an out-of-workspace /worktree-enter target must be refused");
    assert!(
        err.to_string().contains("outside the workspace"),
        "unexpected error: {err}"
    );
    // No cwd change, no state change: the refusal is a pure gate.
    assert_eq!(
        std::env::current_dir().unwrap().canonicalize().unwrap(),
        before.canonicalize().unwrap()
    );
    assert!(!crate::tools::git_worktree::is_in_worktree());
    assert!(crate::tools::git_worktree::get_current_worktree().is_none());
}

#[test]
fn test_enter_worktree_dir_allows_in_workspace_target() {
    let _g = crate::test_support::CwdGuard::hold();
    crate::tools::file::reset_safety_config_for_tests();
    reset_worktree_state();
    let before = std::env::current_dir().unwrap();

    let tmp = tempfile::Builder::new()
        .tempdir_in(std::env::current_dir().unwrap())
        .unwrap();
    let entered = crate::tools::git_worktree::enter_worktree_dir(tmp.path().to_path_buf())
        .expect("an in-workspace /worktree-enter target must be allowed");
    assert_eq!(entered, tmp.path().to_path_buf());
    assert!(crate::tools::git_worktree::is_in_worktree());

    crate::tools::git_worktree::exit_worktree_dir().unwrap();
    assert_eq!(
        std::env::current_dir().unwrap().canonicalize().unwrap(),
        before.canonicalize().unwrap()
    );
    assert!(!crate::tools::git_worktree::is_in_worktree());
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

#[test]
fn test_worktree_state_new() {
    let state = WorktreeState::new();
    assert!(!state.is_in_worktree());
    assert!(state.current().is_none());
    assert!(state.root().is_none());
}

#[tokio::test]
async fn test_list_worktrees_execute() {
    // The tool runs `git worktree list` in the process cwd — a shared
    // global. Hold the cwd guard so a concurrent test that `chdir`s into
    // a temp dir can't make git fail underneath us (see test_support.rs).
    let _g = crate::test_support::CwdGuard::hold();
    let tool = ListWorktreesTool::new();
    let args = serde_json::json!({});

    // This will work in a git repo
    let result = tool.execute(args).await;
    assert!(result.is_ok());

    let output = result.unwrap();
    assert!(output.get("worktrees").is_some());
    assert!(output.get("count").is_some());
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
async fn test_exit_worktree_not_in_worktree() {
    // Hold the shared cwd lock while resetting the process-global worktree
    // state so this cannot interleave with the enter/exit tests that mutate it.
    let _g = crate::test_support::CwdGuard::hold();
    // Reset state to ensure we're not in a worktree
    {
        let mut state = WORKTREE_STATE.lock().unwrap();
        *state = WorktreeState::new();
    }

    let tool = ExitWorktreeTool::new();
    let args = serde_json::json!({});

    let result = tool.execute(args).await;
    // Should fail since we're not in a worktree
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("Not currently in a worktree"));
}

#[test]
fn test_is_in_worktree_initially_false() {
    // Reset state. Hold the shared cwd lock: the worktree session state is
    // process-global, and enter/exit tests below mutate it together with the
    // process cwd.
    let _g = crate::test_support::CwdGuard::hold();
    {
        let mut state = WORKTREE_STATE.lock().unwrap();
        *state = WorktreeState::new();
    }

    assert!(!is_in_worktree());
}

#[test]
fn test_get_current_worktree_initially_none() {
    // Reset state (see note in test_is_in_worktree_initially_false)
    let _g = crate::test_support::CwdGuard::hold();
    {
        let mut state = WORKTREE_STATE.lock().unwrap();
        *state = WorktreeState::new();
    }

    assert!(get_current_worktree().is_none());
}

/// Create an isolated git repository and chdir into it, returning the cwd
/// guard (which restores the original directory on drop). Mirrors the helper
/// in tests/unit/tools/git/git_test.rs.
fn isolated_git_repo() -> (crate::test_support::CwdGuard, tempfile::TempDir) {
    let dir = tempfile::TempDir::new().unwrap();
    let guard = crate::test_support::CwdGuard::enter(dir.path());
    fn git(args: &[&str]) {
        std::process::Command::new("git")
            .args(args)
            .status()
            .unwrap();
    }
    git(&["init", "-q"]);
    git(&["config", "user.email", "t@t"]);
    git(&["config", "user.name", "t"]);
    std::fs::write(dir.path().join("f.txt"), "x").unwrap();
    git(&["add", "-A"]);
    git(&["commit", "-qm", "base"]);
    (guard, dir)
}

/// Reset the process-global worktree state. Must be called while holding the
/// shared cwd lock (see `CwdGuard`).
fn reset_worktree_state() {
    let mut state = WORKTREE_STATE.lock().unwrap();
    *state = WorktreeState::new();
}

#[test]
fn test_push_pop_roundtrip_restores_cwd() {
    let _g = crate::test_support::CwdGuard::hold();
    let mut state = WorktreeState::new();
    let original = std::env::current_dir().unwrap();
    let tmp = tempfile::tempdir().unwrap();
    let canonical_tmp = tmp.path().canonicalize().unwrap();

    state.push_worktree(tmp.path().to_path_buf()).unwrap();
    assert_eq!(
        std::env::current_dir().unwrap().canonicalize().unwrap(),
        canonical_tmp
    );
    assert!(state.is_in_worktree());

    state.pop_worktree(false).unwrap();
    assert_eq!(
        std::env::current_dir().unwrap().canonicalize().unwrap(),
        original.canonicalize().unwrap()
    );
    assert!(!state.is_in_worktree());
}

#[test]
fn test_push_failure_restores_cwd_and_leaves_state_consistent() {
    let _g = crate::test_support::CwdGuard::hold();
    let mut state = WorktreeState::new();
    let original = std::env::current_dir().unwrap();
    let tmp = tempfile::tempdir().unwrap();
    let missing = tmp.path().join("does-not-exist");

    // Entering a worktree that does not exist on disk fails. `initialize()`
    // records the pre-existing directory as the stack root, but the failed
    // push must not change the process cwd and must not record a worktree.
    let res = state.push_worktree(missing);
    assert!(res.is_err());
    assert_eq!(
        std::env::current_dir().unwrap().canonicalize().unwrap(),
        original.canonicalize().unwrap()
    );
    assert!(!state.is_in_worktree());
    assert!(state.current_worktree.is_none());
    assert_eq!(state.directory_stack.len(), 1); // only the recorded root
}

#[tokio::test]
async fn test_enter_exit_dir_roundtrip_restores_cwd() {
    let _g = crate::test_support::CwdGuard::hold();
    crate::tools::file::reset_safety_config_for_tests();
    reset_worktree_state();
    let original = std::env::current_dir().unwrap();
    // In-workspace tempdir: `enter_worktree_dir` validates the target against
    // the workspace path policy (2026-09-21), so the tempdir must live under
    // the workspace root — an out-of-workspace target is refused.
    let tmp = tempfile::Builder::new()
        .tempdir_in(std::env::current_dir().unwrap())
        .unwrap();
    let canonical_tmp = tmp.path().canonicalize().unwrap();

    let entered = crate::tools::git_worktree::enter_worktree_dir(tmp.path().to_path_buf()).unwrap();
    assert_eq!(entered, tmp.path().to_path_buf());
    assert_eq!(
        std::env::current_dir().unwrap().canonicalize().unwrap(),
        canonical_tmp
    );
    assert!(crate::tools::git_worktree::is_in_worktree());

    let (restored, previous) = crate::tools::git_worktree::exit_worktree_dir().unwrap();
    assert_eq!(
        restored.canonicalize().unwrap(),
        original.canonicalize().unwrap()
    );
    assert_eq!(previous.as_deref(), Some(tmp.path()));
    assert_eq!(
        std::env::current_dir().unwrap().canonicalize().unwrap(),
        original.canonicalize().unwrap()
    );
    assert!(!crate::tools::git_worktree::is_in_worktree());
}

#[tokio::test]
async fn test_exit_dir_without_enter_errors_and_preserves_cwd() {
    let _g = crate::test_support::CwdGuard::hold();
    reset_worktree_state();
    let original = std::env::current_dir().unwrap();

    let res = crate::tools::git_worktree::exit_worktree_dir();
    assert!(res.is_err());
    assert!(res
        .unwrap_err()
        .to_string()
        .contains("Not currently in a worktree"));
    assert_eq!(
        std::env::current_dir().unwrap().canonicalize().unwrap(),
        original.canonicalize().unwrap()
    );
}

#[tokio::test]
async fn test_concurrent_observer_never_observes_stranded_cwd() {
    let _g = crate::test_support::CwdGuard::hold();
    crate::tools::file::reset_safety_config_for_tests();
    reset_worktree_state();
    let original = std::env::current_dir().unwrap().canonicalize().unwrap();
    // In-workspace tempdir — `enter_worktree_dir` enforces the workspace path
    // policy (2026-09-21), so the enter/exit probe paths must live under the
    // workspace root.
    let tmp = tempfile::Builder::new()
        .tempdir_in(std::env::current_dir().unwrap())
        .unwrap();
    let canonical_tmp = tmp.path().canonicalize().unwrap();
    let missing = tmp.path().join("does-not-exist");

    // A background task that — like the concurrent file_read/file_write path
    // resolution the mutation used to corrupt — samples the process cwd while
    // the main task runs a failing enter and a full enter/exit cycle.
    let observer = tokio::spawn(async move {
        let mut seen = std::collections::BTreeSet::new();
        for _ in 0..64 {
            seen.insert(std::env::current_dir().unwrap().canonicalize().unwrap());
            tokio::task::yield_now().await;
        }
        seen
    });

    // Error path: entering a worktree whose directory does not exist fails
    // without changing the process cwd.
    assert!(crate::tools::git_worktree::enter_worktree_dir(missing).is_err());
    assert_eq!(
        std::env::current_dir().unwrap().canonicalize().unwrap(),
        original
    );
    tokio::task::yield_now().await;

    // Success path: full enter/exit cycle; the cwd is restored on exit.
    crate::tools::git_worktree::enter_worktree_dir(tmp.path().to_path_buf()).unwrap();
    assert!(crate::tools::git_worktree::is_in_worktree());
    tokio::task::yield_now().await;
    crate::tools::git_worktree::exit_worktree_dir().unwrap();
    assert_eq!(
        std::env::current_dir().unwrap().canonicalize().unwrap(),
        original
    );
    assert!(!crate::tools::git_worktree::is_in_worktree());
    tokio::task::yield_now().await;

    let seen = observer.await.unwrap();
    // The observer only ever resolved against directories the worktree state
    // knew about: the original directory or the temporarily entered worktree.
    // Neither the missing enter target nor any other unknown directory could
    // ever be observed as the process cwd.
    assert!(
        seen.iter().all(|c| c == &original || c == &canonical_tmp),
        "observer saw an unexpected cwd: {:?}",
        seen
    );
    assert!(seen.contains(&original));
}

/// Regression (2026-09-21 Pass-3 review): the worktree feature switches the
/// PROCESS current directory (`std::env::set_current_dir`), a single global
/// across every Tokio worker thread. A relative-path operation running
/// concurrently on another thread — a file tool resolving a relative path, a
/// path validation, a cargo/git invocation — must stay coherent through a full
/// enter/exit cycle: it may only ever resolve against a directory the worktree
/// state accounts for (the original workspace root or the entered worktree,
/// never a torn or unknown intermediate), it must not error, and when the
/// cycle is over the process cwd must be EXACTLY the original again — no
/// worktree residue that would silently retarget the next relative path.
///
/// Transitions are serialized by the process-wide transition lock
/// (WORKTREE_TRANSITION_LOCK in src/tools/git_worktree.rs) taken around the
/// cwd switch + bookkeeping, so the enter and exit never interleave with each
/// other and the restore always lands on the recorded previous directory.
#[tokio::test]
async fn test_concurrent_relative_path_operation_through_enter_exit_cycle() {
    let _g = crate::test_support::CwdGuard::hold();
    crate::tools::file::reset_safety_config_for_tests();
    reset_worktree_state();
    let original = std::env::current_dir().unwrap().canonicalize().unwrap();

    // A task that — like a concurrent file tool / cargo / git invocation on
    // another thread — resolves a RELATIVE path against the process cwd in a
    // loop, recording the directory each resolution ran against.
    let observer = tokio::spawn(async move {
        let mut seen: Vec<std::path::PathBuf> = Vec::new();
        for _ in 0..200 {
            let cwd = std::env::current_dir().unwrap();
            // The relative-path operation: a read-side join + existence
            // probe. A torn cwd would resolve this against a directory the
            // worktree state does not know about (and an already-restored
            // exit would resolve it against the original — both accounted
            // for by the assertion below).
            let relative = cwd.join("sw-w2-relative-probe");
            let _ = std::fs::metadata(&relative);
            seen.push(cwd);
            tokio::task::yield_now().await;
        }
        seen
    });
    tokio::task::yield_now().await;

    let tmp = tempfile::Builder::new()
        .tempdir_in(std::env::current_dir().unwrap())
        .unwrap();
    let canonical_tmp = tmp.path().canonicalize().unwrap();

    // Full enter/exit cycle on the main task.
    crate::tools::git_worktree::enter_worktree_dir(tmp.path().to_path_buf())
        .expect("enter must succeed");
    assert!(crate::tools::git_worktree::is_in_worktree());
    tokio::task::yield_now().await;
    let (restored, _prev) =
        crate::tools::git_worktree::exit_worktree_dir().expect("exit must succeed");
    assert_eq!(
        restored.canonicalize().unwrap(),
        original,
        "the cycle restores the original directory"
    );

    // After the cycle the process cwd is EXACTLY the original — no residue.
    assert_eq!(
        std::env::current_dir().unwrap().canonicalize().unwrap(),
        original,
        "no worktree residue may survive the cycle"
    );

    // Every concurrent relative-path operation ran against a coherent,
    // state-accounted directory (original or entered worktree) — and the
    // operation never failed or observed anything in between.
    let seen = observer.await.unwrap();
    assert_eq!(
        seen.len(),
        200,
        "the relative-path operation ran unaffected"
    );
    assert!(
        seen.iter()
            .all(|c| c.canonicalize().unwrap() == original
                || c.canonicalize().unwrap() == canonical_tmp),
        "relative-path op observed an unexpected cwd: {:?}",
        seen
    );
}

#[tokio::test]
async fn test_enter_exit_worktree_tool_restores_cwd() {
    let (_g, dir) = isolated_git_repo();
    reset_worktree_state();
    let original = std::env::current_dir().unwrap();
    let worktree = dir.path().join("wt");

    // These tests exercise CWD-restore semantics, not path policy — pin an
    // explicit permissive config so the ambient (global) safety config and
    // the concurrent-cwd state cannot route the tempdir worktree path into
    // a PathNotAllowed refusal (2026-09-21: validate_path became real, and
    // the pre-existing same-thread cwd race surfaced through it).
    let permissive = SafetyConfig {
        allowed_paths: vec!["/**".to_string()],
        ..SafetyConfig::default()
    };
    let enter = EnterWorktreeTool::with_safety_config(permissive.clone());
    let res = enter
        .execute(serde_json::json!({ "path": worktree.to_string_lossy() }))
        .await;
    assert!(res.is_ok(), "enter_worktree failed: {:?}", res.err());
    assert!(crate::tools::git_worktree::is_in_worktree());
    assert_eq!(
        std::env::current_dir().unwrap().canonicalize().unwrap(),
        worktree.canonicalize().unwrap()
    );
    // The global state agrees with the process cwd: current worktree is the
    // one just entered.
    assert_eq!(
        crate::tools::git_worktree::get_current_worktree().as_deref(),
        Some(worktree.as_path())
    );

    let exit = ExitWorktreeTool::new();
    let res = exit.execute(serde_json::json!({ "remove": false })).await;
    assert!(res.is_ok(), "exit_worktree failed: {:?}", res.err());
    assert_eq!(
        std::env::current_dir().unwrap().canonicalize().unwrap(),
        original.canonicalize().unwrap()
    );
    assert!(!crate::tools::git_worktree::is_in_worktree());
}

#[tokio::test]
async fn test_enter_worktree_tool_error_preserves_cwd() {
    let (_g, dir) = isolated_git_repo();
    reset_worktree_state();
    let blocker = dir.path().join("blocker");
    std::fs::write(&blocker, "x").unwrap();
    let original = std::env::current_dir().unwrap();

    // Worktree path under a regular file: `create_dir_all` fails before any
    // cwd change, and the failure must leave both the cwd and the state
    // untouched. Pinned permissive config — see the enter/exit test above.
    let permissive = SafetyConfig {
        allowed_paths: vec!["/**".to_string()],
        ..SafetyConfig::default()
    };
    let enter = EnterWorktreeTool::with_safety_config(permissive);
    let res = enter
        .execute(serde_json::json!({ "path": blocker.join("child").to_string_lossy() }))
        .await;
    assert!(res.is_err());
    assert_eq!(
        std::env::current_dir().unwrap().canonicalize().unwrap(),
        original.canonicalize().unwrap()
    );
    assert!(!crate::tools::git_worktree::is_in_worktree());
    assert!(crate::tools::git_worktree::get_current_worktree().is_none());
}

#[test]
fn test_pop_failure_when_previous_dir_deleted_is_propagated() {
    let _g = crate::test_support::CwdGuard::hold();
    let mut state = WorktreeState::new();
    let previous = tempfile::tempdir().unwrap();
    let worktree = tempfile::TempDir::new().unwrap();
    let canonical_worktree = worktree.path().canonicalize().unwrap();

    // Enter a worktree from `previous`: the level's guard records
    // `previous` as the directory the exit must restore to.
    state.push_worktree(previous.path().to_path_buf()).unwrap();
    state.push_worktree(worktree.path().to_path_buf()).unwrap();
    assert_eq!(
        std::env::current_dir().unwrap().canonicalize().unwrap(),
        canonical_worktree
    );

    // Delete the directory the exit would restore to.
    drop(previous);

    // Exit must FAIL rather than silently report success while the cwd is
    // stranded (the previous behaviour: Drop only logged the failed restore).
    let res = state.pop_worktree(false);
    assert!(res.is_err());
    let err_msg = res.err().unwrap().to_string();
    assert!(
        err_msg.contains("Failed to restore"),
        "unexpected error: {:?}",
        err_msg
    );

    // The failure left state and cwd consistent: still reported inside the
    // worktree, still rooted in it — not popped underneath the caller.
    assert!(state.is_in_worktree());
    assert_eq!(
        std::env::current_dir().unwrap().canonicalize().unwrap(),
        canonical_worktree
    );
}

#[test]
fn test_nested_pop_restores_to_previous_level_not_stack_root() {
    let _g = crate::test_support::CwdGuard::hold();
    let mut state = WorktreeState::new();
    let original = std::env::current_dir().unwrap();
    let outer = tempfile::tempdir().unwrap();
    let inner = tempfile::tempdir().unwrap();
    let canonical_outer = outer.path().canonicalize().unwrap();
    let canonical_inner = inner.path().canonicalize().unwrap();

    state.push_worktree(outer.path().to_path_buf()).unwrap();
    state.push_worktree(inner.path().to_path_buf()).unwrap();
    assert_eq!(
        std::env::current_dir().unwrap().canonicalize().unwrap(),
        canonical_inner
    );

    // Exiting the inner worktree restores the process cwd to the OUTER
    // worktree — the directory that was current before entering — and must
    // report that directory, not the stack root (previously the exit returned
    // the stack's first() entry regardless of what was actually restored).
    let (restored, _removed) = state.pop_worktree(false).unwrap();
    assert_eq!(restored.canonicalize().unwrap(), canonical_outer);
    assert_eq!(
        std::env::current_dir().unwrap().canonicalize().unwrap(),
        canonical_outer
    );
    assert!(state.is_in_worktree()); // still inside the outer worktree

    // The final exit restores the original directory and reports it.
    let (restored, _removed) = state.pop_worktree(false).unwrap();
    assert_eq!(
        restored.canonicalize().unwrap(),
        original.canonicalize().unwrap()
    );
    assert_eq!(
        std::env::current_dir().unwrap().canonicalize().unwrap(),
        original.canonicalize().unwrap()
    );
    assert!(!state.is_in_worktree());
}
