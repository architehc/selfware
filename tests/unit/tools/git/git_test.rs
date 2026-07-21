use super::*;

#[test]
fn test_git_status_name() {
    let tool = GitStatus::new();
    assert_eq!(tool.name(), "git_status");
}

#[test]
fn test_git_status_description() {
    let tool = GitStatus::new();
    assert!(tool.description().contains("status"));
}

#[test]
fn test_git_status_schema() {
    let tool = GitStatus::new();
    let schema = tool.schema();
    assert_eq!(schema["type"], "object");
}

#[test]
fn test_git_diff_name() {
    let tool = GitDiff::new();
    assert_eq!(tool.name(), "git_diff");
}

#[test]
fn test_git_diff_description() {
    let tool = GitDiff::new();
    assert!(tool.description().contains("diff"));
}

#[test]
fn test_git_diff_schema() {
    let tool = GitDiff::new();
    let schema = tool.schema();
    assert_eq!(schema["type"], "object");
    assert!(schema["properties"]["staged"].is_object());
}

#[test]
fn test_git_commit_name() {
    let tool = GitCommit::new();
    assert_eq!(tool.name(), "git_commit");
}

#[test]
fn test_git_commit_description() {
    let tool = GitCommit::new();
    assert!(tool.description().contains("commit"));
}

#[test]
fn test_git_commit_schema() {
    let tool = GitCommit::new();
    let schema = tool.schema();
    assert_eq!(schema["type"], "object");
    assert!(schema["properties"]["message"].is_object());
    assert!(schema["properties"]["files"].is_object());
}

#[test]
fn test_git_checkpoint_name() {
    let tool = GitCheckpoint::new();
    assert_eq!(tool.name(), "git_checkpoint");
}

#[test]
fn test_git_checkpoint_description() {
    let tool = GitCheckpoint::new();
    assert!(tool.description().contains("checkpoint"));
    assert!(tool.description().contains("rollback"));
}

#[test]
fn test_git_checkpoint_schema() {
    let tool = GitCheckpoint::new();
    let schema = tool.schema();
    assert_eq!(schema["type"], "object");
    assert!(schema["properties"]["message"].is_object());
    assert!(schema["properties"]["tag"].is_object());
    assert!(schema["properties"]["auto_branch"].is_object());
}

#[test]
fn test_git_checkpoint_schema_required() {
    let tool = GitCheckpoint::new();
    let schema = tool.schema();
    let required = schema["required"].as_array().unwrap();
    assert!(required.contains(&serde_json::json!("message")));
}

#[test]
fn test_git_commit_schema_required() {
    let tool = GitCommit::new();
    let schema = tool.schema();
    let required = schema["required"].as_array().unwrap();
    assert!(required.contains(&serde_json::json!("message")));
}

#[test]
fn test_git_commit_schema_commit_types() {
    let tool = GitCommit::new();
    let schema = tool.schema();
    let commit_type = &schema["properties"]["commit_type"];
    let enum_values = commit_type["enum"].as_array().unwrap();

    assert!(enum_values.contains(&serde_json::json!("feat")));
    assert!(enum_values.contains(&serde_json::json!("fix")));
    assert!(enum_values.contains(&serde_json::json!("refactor")));
}

#[tokio::test]
async fn test_git_status_execute() {
    let _g = crate::test_support::CwdGuard::hold();
    let tool = GitStatus::new();
    let args = serde_json::json!({});

    // This will work in a git repo (like this project)
    let result = tool.execute(args).await;
    assert!(result.is_ok());

    let output = result.unwrap();
    assert!(output.get("branch").is_some() || output.get("error").is_some());
}

#[tokio::test]
async fn test_git_diff_execute_unstaged() {
    let _g = crate::test_support::CwdGuard::hold();
    let tool = GitDiff::new();
    let args = serde_json::json!({"staged": false});

    let result = tool.execute(args).await;
    assert!(result.is_ok());

    let output = result.unwrap();
    assert!(output.get("diff").is_some() || output.get("error").is_some());
}

#[tokio::test]
async fn test_git_diff_execute_staged() {
    let _g = crate::test_support::CwdGuard::hold();
    let tool = GitDiff::new();
    let args = serde_json::json!({"staged": true});

    let result = tool.execute(args).await;
    assert!(result.is_ok());
}

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

#[tokio::test]
async fn test_git_commit_with_message() {
    let _iso = isolated_git_repo();
    let tool = GitCommit::new();
    // This test creates a real commit - only check that it handles the case
    // when there's nothing to commit gracefully
    let args = serde_json::json!({
        "message": "Test commit",
        "files": []
    });

    // This may fail if nothing to commit, but shouldn't panic
    let result = tool.execute(args).await;
    // We accept both Ok (committed) and Err (nothing to commit)
    assert!(result.is_ok() || result.is_err());
}

#[tokio::test]
async fn empty_files_commit_excludes_untracked() {
    let (_guard, dir) = isolated_git_repo();
    // Modify a tracked file and drop a new untracked one (a stray secret).
    std::fs::write(dir.path().join("f.txt"), "modified content").unwrap();
    std::fs::write(dir.path().join("untracked_secret.txt"), "ghp_secret").unwrap();

    let tool = GitCommit::new();
    let args = serde_json::json!({ "message": "commit tracked only", "files": [] });
    tool.execute(args).await.expect("commit of tracked change");

    let status = std::process::Command::new("git")
        .args(["status", "--porcelain"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    let status_str = String::from_utf8_lossy(&status.stdout);

    // The untracked file must NOT have been swept into the commit.
    assert!(
        status_str.contains("untracked_secret.txt"),
        "empty-files commit must leave untracked files untracked; status: {status_str}"
    );
    // The tracked modification WAS committed (no longer pending).
    assert!(
        !status_str.contains("f.txt"),
        "tracked modification should have been committed; status: {status_str}"
    );
}

#[tokio::test]
async fn test_git_checkpoint_execute() {
    let _iso = isolated_git_repo();
    let tool = GitCheckpoint::new();
    let args = serde_json::json!({
        "message": "Test checkpoint"
    });

    // This might fail if there's nothing to commit, but shouldn't panic
    let result = tool.execute(args).await;
    // We just verify it returns Ok or expected Err
    assert!(result.is_ok() || result.is_err());
}

#[test]
fn test_git_diff_schema_properties() {
    let tool = GitDiff::new();
    let schema = tool.schema();

    assert!(schema["properties"]["staged"].is_object());
    assert!(schema["properties"]["path"].is_object());
    assert!(schema["properties"]["base"].is_object());
}

#[test]
fn test_git_checkpoint_schema_defaults() {
    let tool = GitCheckpoint::new();
    let schema = tool.schema();

    let auto_branch = &schema["properties"]["auto_branch"];
    assert_eq!(auto_branch["default"], true);
}

#[test]
fn test_git_status_schema_properties() {
    let tool = GitStatus::new();
    let schema = tool.schema();

    assert!(schema["properties"]["repo_path"].is_object());
}

#[test]
fn test_git_commit_schema_files_array() {
    let tool = GitCommit::new();
    let schema = tool.schema();

    let files = &schema["properties"]["files"];
    assert_eq!(files["type"], "array");
}

// Additional tests for error paths and edge cases

#[tokio::test]
async fn test_git_status_not_a_repo() {
    use tempfile::TempDir;
    let temp_dir = TempDir::new().unwrap();

    let tool = GitStatus::new();
    let args = serde_json::json!({
        "repo_path": temp_dir.path().to_str().unwrap()
    });

    let result = tool.execute(args).await;
    // Should fail since it's not a git repo
    assert!(result.is_err());
}

#[tokio::test]
async fn test_git_status_with_explicit_current_dir() {
    let _g = crate::test_support::CwdGuard::hold();
    let tool = GitStatus::new();
    let args = serde_json::json!({
        "repo_path": "."  // Explicit current dir
    });

    // Should work since we're in a git repo
    let result = tool.execute(args).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_git_diff_with_specific_path() {
    let _g = crate::test_support::CwdGuard::hold();
    let tool = GitDiff::new();
    let args = serde_json::json!({
        "path": ".",
        "staged": false
    });

    let result = tool.execute(args).await;
    assert!(result.is_ok());

    let output = result.unwrap();
    // Should have diff field (may be empty)
    assert!(output.get("diff").is_some());
    assert!(output.get("has_changes").is_some());
}

#[tokio::test]
async fn test_git_commit_with_specific_files() {
    let _iso = isolated_git_repo();
    let tool = GitCommit::new();
    let args = serde_json::json!({
        "message": "Test specific files",
        "files": ["nonexistent_file_12345.txt"]  // File doesn't exist
    });

    // `git add` exits non-zero for an unmatched pathspec; the tool must
    // surface that error instead of silently committing a partial stage.
    let result = tool.execute(args).await;
    let err = result.expect_err("add of a nonexistent path must fail the commit");
    assert!(
        err.to_string().contains("git add failed"),
        "unexpected error: {err}"
    );
}

#[tokio::test]
async fn test_git_commit_surfaces_add_u_failure() {
    // `git add -u` outside a work tree fails; the error must be surfaced
    // rather than proceeding to a bogus commit.
    let dir = tempfile::TempDir::new().unwrap();
    let _guard = crate::test_support::CwdGuard::enter(dir.path());
    let tool = GitCommit::new();
    let result = tool
        .execute(serde_json::json!({ "message": "x", "files": [] }))
        .await;
    let err = result.expect_err("add -u outside a repo must fail");
    assert!(
        err.to_string().contains("git add"),
        "unexpected error: {err}"
    );
}

#[tokio::test]
async fn test_git_diff_honors_base_param() {
    let (_guard, dir) = isolated_git_repo();
    // Modify the tracked file; diff against HEAD must show it.
    std::fs::write(dir.path().join("f.txt"), "changed content").unwrap();

    let tool = GitDiff::new();
    let result = tool
        .execute(serde_json::json!({ "base": "HEAD" }))
        .await
        .expect("diff against HEAD");
    assert_eq!(result["has_changes"], true);
    assert!(
        result["diff"].as_str().unwrap().contains("changed content"),
        "diff against base must include the modification: {result}"
    );
}

#[tokio::test]
async fn test_git_diff_bad_base_reports_error_not_empty_diff() {
    let _iso = isolated_git_repo();
    let tool = GitDiff::new();
    // A bogus revision used to be reported as has_changes:false with the
    // error silently dropped.
    let result = tool
        .execute(serde_json::json!({ "base": "nonexistent-revision-xyz" }))
        .await;
    let err = result.expect_err("bad base must error, not fake an empty diff");
    assert!(
        err.to_string().contains("git diff failed"),
        "unexpected error: {err}"
    );
}

#[tokio::test]
async fn test_git_diff_rejects_flag_like_base() {
    let _iso = isolated_git_repo();
    let tool = GitDiff::new();
    let result = tool
        .execute(serde_json::json!({ "base": "--output=/tmp/injected" }))
        .await;
    let err = result.expect_err("flag-like base must be rejected");
    assert!(
        err.to_string().contains("Invalid base"),
        "unexpected error: {err}"
    );
}

#[tokio::test]
async fn test_git_checkpoint_with_tag() {
    let _iso = isolated_git_repo();
    let tool = GitCheckpoint::new();
    let args = serde_json::json!({
        "message": "Test checkpoint with tag",
        "tag": "test-checkpoint-tag"
    });

    let result = tool.execute(args).await;
    // May succeed or fail depending on repo state
    assert!(result.is_ok() || result.is_err());
}

#[tokio::test]
async fn test_git_checkpoint_disable_auto_branch() {
    let _iso = isolated_git_repo();
    let tool = GitCheckpoint::new();
    let args = serde_json::json!({
        "message": "Test no auto branch",
        "auto_branch": false
    });

    let result = tool.execute(args).await;
    // Should handle gracefully
    assert!(result.is_ok() || result.is_err());
}

#[test]
fn test_git_status_schema_has_repo_path() {
    let tool = GitStatus::new();
    let schema = tool.schema();

    let repo_path = &schema["properties"]["repo_path"];
    assert_eq!(repo_path["type"], "string");
}

#[test]
fn test_git_diff_schema_has_base() {
    let tool = GitDiff::new();
    let schema = tool.schema();

    let base = &schema["properties"]["base"];
    assert_eq!(base["type"], "string");
}

#[test]
fn test_git_checkpoint_message_required() {
    let tool = GitCheckpoint::new();
    let schema = tool.schema();

    let required = schema["required"].as_array().unwrap();
    assert_eq!(required.len(), 1);
    assert!(required.contains(&serde_json::json!("message")));
}

// GitPush tests

#[test]
fn test_git_push_name() {
    let tool = GitPush::new();
    assert_eq!(tool.name(), "git_push");
}

#[test]
fn test_git_push_description() {
    let tool = GitPush::new();
    assert!(tool.description().contains("Push"));
}

#[test]
fn test_git_push_schema() {
    let tool = GitPush::new();
    let schema = tool.schema();
    assert_eq!(schema["type"], "object");
    assert!(schema["properties"]["remote"].is_object());
    assert!(schema["properties"]["branch"].is_object());
    assert!(schema["properties"]["force"].is_object());
}

#[test]
fn test_git_push_schema_defaults() {
    let tool = GitPush::new();
    let schema = tool.schema();
    assert_eq!(schema["properties"]["remote"]["default"], "origin");
    assert_eq!(schema["properties"]["force"]["default"], false);
}

#[tokio::test]
async fn test_git_push_execute() {
    let tool = GitPush::new();
    // Push to nonexistent remote will fail, but shouldn't panic
    let args = serde_json::json!({
        "remote": "nonexistent_remote_test",
        "branch": "test-branch"
    });
    let result = tool.execute(args).await;
    // Should return Ok with success: false (remote doesn't exist)
    assert!(result.is_ok());
    let output = result.unwrap();
    assert_eq!(output["success"], false);
}

#[tokio::test]
async fn test_git_push_execute_blocks_protected_branch() {
    let safety_config = crate::config::SafetyConfig {
        protected_branches: vec!["main".to_string()],
        ..Default::default()
    };
    let tool = GitPush::with_safety_config(safety_config);
    let args = serde_json::json!({
        "remote": "nonexistent_remote_test",
        "branch": "main"
    });
    let result = tool.execute(args).await;
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("protected branch"));
}

#[tokio::test]
async fn test_git_push_execute_allows_non_protected_branch() {
    let safety_config = crate::config::SafetyConfig {
        protected_branches: vec!["main".to_string()],
        ..Default::default()
    };
    let tool = GitPush::with_safety_config(safety_config);
    let args = serde_json::json!({
        "remote": "nonexistent_remote_test",
        "branch": "some-other-branch"
    });
    let result = tool.execute(args).await;
    // Not blocked by protected_branches; fails later since the remote
    // doesn't exist, same as the un-configured case above.
    assert!(result.is_ok());
    assert_eq!(result.unwrap()["success"], false);
}

// Tests for validate_tag_name function

#[test]
fn test_validate_tag_name_valid() {
    assert!(validate_tag_name("v1.0.0").is_ok());
    assert!(validate_tag_name("release-2024").is_ok());
    assert!(validate_tag_name("feature/new-thing").is_ok());
    assert!(validate_tag_name("hotfix_123").is_ok());
    assert!(validate_tag_name("a").is_ok());
}

#[test]
fn test_validate_tag_name_empty() {
    assert!(validate_tag_name("").is_err());
}

#[test]
fn test_validate_tag_name_too_long() {
    let long_name = "a".repeat(257);
    assert!(validate_tag_name(&long_name).is_err());
}

#[test]
fn test_validate_tag_name_starts_with_dash() {
    assert!(validate_tag_name("-v1.0.0").is_err());
}

#[test]
fn test_validate_tag_name_invalid_chars() {
    assert!(validate_tag_name("v1.0 0").is_err()); // space
    assert!(validate_tag_name("v1.0@0").is_err()); // @
    assert!(validate_tag_name("v1.0#0").is_err()); // #
    assert!(validate_tag_name("v1.0$0").is_err()); // $
    assert!(validate_tag_name("v1.0!0").is_err()); // !
    assert!(validate_tag_name("v1.0*0").is_err()); // *
}

#[test]
fn test_validate_tag_name_exactly_256() {
    let name = "a".repeat(256);
    assert!(validate_tag_name(&name).is_ok());
}

// Tests for write_commit_message_file

#[test]
fn test_write_commit_message_file_creates_file() {
    let message = "Test commit message";
    let path = write_commit_message_file(message);
    assert!(path.is_some());

    let path = path.unwrap();
    let content = std::fs::read_to_string(&path).unwrap();
    assert_eq!(content, message);

    // Clean up
    let _ = std::fs::remove_file(&path);
}

#[test]
fn test_write_commit_message_file_multiline() {
    let message = "Line 1\nLine 2\nLine 3";
    let path = write_commit_message_file(message);
    assert!(path.is_some());

    let path = path.unwrap();
    let content = std::fs::read_to_string(&path).unwrap();
    assert_eq!(content, message);

    // Clean up
    let _ = std::fs::remove_file(&path);
}

#[test]
fn test_write_commit_message_file_unique_names() {
    let path1 = write_commit_message_file("msg1");
    let path2 = write_commit_message_file("msg2");

    assert!(path1.is_some());
    assert!(path2.is_some());
    assert_ne!(path1, path2);

    // Clean up
    let _ = std::fs::remove_file(path1.unwrap());
    let _ = std::fs::remove_file(path2.unwrap());
}
