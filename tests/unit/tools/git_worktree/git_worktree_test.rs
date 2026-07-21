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
    // Reset state
    {
        let mut state = WORKTREE_STATE.lock().unwrap();
        *state = WorktreeState::new();
    }

    assert!(!is_in_worktree());
}

#[test]
fn test_get_current_worktree_initially_none() {
    // Reset state
    {
        let mut state = WORKTREE_STATE.lock().unwrap();
        *state = WorktreeState::new();
    }

    assert!(get_current_worktree().is_none());
}
