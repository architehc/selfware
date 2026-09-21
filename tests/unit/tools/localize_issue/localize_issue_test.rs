use super::*;
use std::fs;
use tempfile::TempDir;

/// The tool now enforces the workspace path policy on `repo_path`; tests using
/// temp-dir fixtures outside the workspace allow those fixtures explicitly
/// (mirroring the file tool tests).
fn permissive_safety_config() -> SafetyConfig {
    SafetyConfig {
        allowed_paths: vec!["/**".to_string()],
        ..SafetyConfig::default()
    }
}

#[test]
fn test_tokenize_filters_short_words() {
    let terms = tokenize("The quick brown fox");
    // "the" is filtered as a stop word.
    assert!(!terms.contains(&"the".to_string()));
    assert!(terms.contains(&"quick".to_string()));
    assert!(terms.contains(&"brown".to_string()));
    assert!(terms.contains(&"fox".to_string()));
    // "a" and "ab" (length <= 2) are filtered.
    let short = tokenize("a ab abc");
    assert!(!short.contains(&"a".to_string()));
    assert!(!short.contains(&"ab".to_string()));
    assert!(short.contains(&"abc".to_string()));
}

#[test]
fn test_localize_issue_finds_relevant_file() {
    let temp_dir = TempDir::new().unwrap();
    let repo = temp_dir.path();

    // Create a dummy source tree.
    fs::create_dir_all(repo.join("src")).unwrap();
    fs::create_dir_all(repo.join("tests")).unwrap();
    fs::write(
        repo.join("src/lib.rs"),
        "pub fn process_data(input: &str) -> String {\n    input.to_string()\n}\n",
    )
    .unwrap();
    fs::write(
        repo.join("src/main.rs"),
        "fn main() {\n    println!(\"hello\");\n}\n",
    )
    .unwrap();
    fs::write(
        repo.join("tests/test_process.rs"),
        "#[test]\nfn test_process_data() {\n    assert_eq!(process_data(\"x\"), \"x\");\n}\n",
    )
    .unwrap();

    let candidates = localize_issue_sync(
        "process_data returns wrong value",
        repo.to_str().unwrap(),
        None,
    )
    .unwrap();
    assert!(!candidates.is_empty(), "should find at least one candidate");

    // The lib.rs file should rank highest because it contains process_data.
    let top_file = &candidates[0].file;
    assert!(
        top_file.contains("lib.rs") || top_file.contains("test_process.rs"),
        "top candidate should be lib.rs or test_process.rs, got {}",
        top_file
    );

    // Check that the candidate has a reasonable reason.
    assert!(!candidates[0].reason.is_empty());
}

#[test]
fn test_localize_issue_empty_query() {
    let temp_dir = TempDir::new().unwrap();
    let candidates = localize_issue_sync("a", temp_dir.path().to_str().unwrap(), None).unwrap();
    assert!(candidates.is_empty());
}

#[test]
fn test_best_function_in_file() {
    let code = "pub fn foo() {}\npub fn bar() {}\npub fn process_data() {}";
    let terms = vec!["process".to_string(), "data".to_string()];
    let best = best_function_in_file(code, &terms);
    assert_eq!(best, "process_data");
}

#[tokio::test]
async fn test_localize_issue_tool_execute() {
    let temp_dir = TempDir::new().unwrap();
    let repo = temp_dir.path();
    fs::create_dir_all(repo.join("src")).unwrap();
    fs::write(repo.join("src/lib.rs"), "pub fn fix_me() {}\n").unwrap();

    let tool = LocalizeIssue::with_safety_config(permissive_safety_config());
    let args = serde_json::json!({
        "issue": "fix_me is broken",
        "repo_path": repo.to_str().unwrap()
    });

    let result = tool.execute(args).await.unwrap();
    let candidates = result["candidates"].as_array().unwrap();
    assert!(!candidates.is_empty());
    assert!(candidates[0]["file"].as_str().unwrap().contains("lib.rs"));
}

/// localize_issue reads every source file under `repo_path` — an
/// out-of-workspace root must be rejected with the path-policy error
/// (regression for the read-only tools bypassing path validation).
#[tokio::test]
async fn test_localize_issue_rejects_out_of_workspace_repo() {
    let tool = LocalizeIssue::with_safety_config(SafetyConfig::default());
    let args = serde_json::json!({
        "issue": "something is broken",
        "repo_path": "/etc"
    });

    let result = tool.execute(args).await;
    let err = result.expect_err("localize_issue on /etc must be rejected");
    let message = err.to_string();
    assert!(
        message.contains("outside working directory")
            || message.contains("not in allowed list")
            || message.contains("protected system path"),
        "expected a path-policy rejection, got: {}",
        message
    );
}

/// P1 regression: localize_issue reads every source file under the repo root —
/// a denied subdirectory inside the allowed root must not be read, so no
/// candidate from it is reported.
#[tokio::test]
async fn test_localize_issue_denied_child_excluded() {
    let temp_dir = TempDir::new().unwrap();
    let repo = temp_dir.path();
    fs::create_dir_all(repo.join("src")).unwrap();
    fs::create_dir_all(repo.join("denied_dir")).unwrap();
    fs::write(repo.join("src/lib.rs"), "pub fn process_broken() {}\n").unwrap();
    // The denied file is a much stronger textual match than the allowed one —
    // if it were read at all, it would rank first.
    fs::write(
        repo.join("denied_dir/lib.rs"),
        "pub fn process_broken() {}\npub fn process_broken() {}\npub fn process_broken() {}\n",
    )
    .unwrap();

    let tool = LocalizeIssue::with_safety_config(SafetyConfig {
        allowed_paths: vec!["/**".to_string()],
        denied_paths: vec!["denied_dir".to_string()],
        ..SafetyConfig::default()
    });
    let args = serde_json::json!({
        "issue": "process_broken is broken",
        "repo_path": repo.to_str().unwrap()
    });

    let result = tool.execute(args).await.unwrap();
    let candidates = result["candidates"].as_array().unwrap();
    assert!(!candidates.is_empty(), "allowed files must still be ranked");
    for c in candidates {
        let file = c["file"].as_str().unwrap();
        assert!(
            !file.contains("denied_dir"),
            "candidate from a denied subdirectory leaked: {}",
            file
        );
    }
}

#[test]
fn test_recent_git_files_with_spaces_and_leading_whitespace() {
    let temp_dir = TempDir::new().unwrap();
    let repo = temp_dir.path();

    let init_ok = std::process::Command::new("git")
        .args(["init"])
        .current_dir(repo)
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if !init_ok {
        return;
    }
    let _ = std::process::Command::new("git")
        .args(["config", "user.email", "test@example.com"])
        .current_dir(repo)
        .status();
    let _ = std::process::Command::new("git")
        .args(["config", "user.name", "Test User"])
        .current_dir(repo)
        .status();

    fs::write(repo.join(" normal.rs"), "fn normal() {}\n").unwrap();
    fs::write(repo.join("file with spaces.rs"), "fn spaces() {}\n").unwrap();

    let _ = std::process::Command::new("git")
        .args(["add", "."])
        .current_dir(repo)
        .status();
    let _ = std::process::Command::new("git")
        .args(["commit", "-m", "init"])
        .current_dir(repo)
        .status();

    let files = recent_git_files(repo.to_str().unwrap());
    assert!(files.contains(" normal.rs"), "must preserve leading space");
    assert!(
        files.contains("file with spaces.rs"),
        "must preserve spaces"
    );
}
