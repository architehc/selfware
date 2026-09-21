use super::*;
use std::fs;
use tempfile::TempDir;

/// Create a permissive `SafetyConfig` for tests that need to access temp dirs.
/// Mirrors the approach used by the file tool tests: the search tools now
/// enforce the workspace path policy, so fixtures outside the workspace need
/// an explicit allow-list.
fn permissive_safety_config() -> SafetyConfig {
    SafetyConfig {
        allowed_paths: vec!["/**".to_string()],
        ..SafetyConfig::default()
    }
}

/// Allow only `root` and its children — used to prove that a symlink escaping
/// `root` is rejected while a symlink staying inside it is fine.
fn restricted_safety_config(root: &std::path::Path) -> SafetyConfig {
    SafetyConfig {
        allowed_paths: vec![format!("{}/**", root.display())],
        ..SafetyConfig::default()
    }
}

#[test]
fn test_grep_search_name() {
    let tool = GrepSearch::new();
    assert_eq!(tool.name(), "grep_search");
}

#[test]
fn test_grep_search_description() {
    let tool = GrepSearch::new();
    assert!(tool.description().contains("regex"));
}

#[test]
fn test_grep_search_schema() {
    let tool = GrepSearch::new();
    let schema = tool.schema();
    assert_eq!(schema["type"], "object");
    assert!(schema["required"]
        .as_array()
        .unwrap()
        .contains(&serde_json::json!("pattern")));
}

#[tokio::test]
async fn test_grep_search_basic() {
    let temp_dir = TempDir::new().unwrap();
    let file_path = temp_dir.path().join("test.txt");
    fs::write(&file_path, "line one\nline two\nline three").unwrap();

    let tool = GrepSearch::with_safety_config(permissive_safety_config());
    let args = serde_json::json!({
        "pattern": "line two",
        "path": temp_dir.path().to_str().unwrap()
    });

    let result = tool.execute(args).await.unwrap();
    let matches = result["matches"].as_array().unwrap();
    assert_eq!(matches.len(), 1);
    assert!(matches[0]["content"].as_str().unwrap().contains("line two"));
}

#[tokio::test]
async fn test_grep_search_case_insensitive() {
    let temp_dir = TempDir::new().unwrap();
    let file_path = temp_dir.path().join("test.txt");
    fs::write(&file_path, "HELLO world").unwrap();

    let tool = GrepSearch::with_safety_config(permissive_safety_config());
    let args = serde_json::json!({
        "pattern": "hello",
        "path": temp_dir.path().to_str().unwrap(),
        "case_insensitive": true
    });

    let result = tool.execute(args).await.unwrap();
    let matches = result["matches"].as_array().unwrap();
    assert_eq!(matches.len(), 1);
}

#[tokio::test]
async fn test_grep_search_with_context() {
    let temp_dir = TempDir::new().unwrap();
    let file_path = temp_dir.path().join("test.txt");
    fs::write(&file_path, "line 1\nline 2\nline 3\nline 4\nline 5").unwrap();

    let tool = GrepSearch::with_safety_config(permissive_safety_config());
    let args = serde_json::json!({
        "pattern": "line 3",
        "path": temp_dir.path().to_str().unwrap(),
        "context_lines": 1
    });

    let result = tool.execute(args).await.unwrap();
    let matches = result["matches"].as_array().unwrap();
    assert_eq!(matches.len(), 1);

    let match_obj = &matches[0];
    let context_before = match_obj["context_before"].as_array().unwrap();
    let context_after = match_obj["context_after"].as_array().unwrap();

    assert_eq!(context_before.len(), 1);
    assert_eq!(context_after.len(), 1);
}

#[tokio::test]
async fn test_grep_search_pagination() {
    let temp_dir = TempDir::new().unwrap();
    let file_path = temp_dir.path().join("test.txt");
    fs::write(&file_path, "match\nmatch\nmatch\nmatch\nmatch").unwrap();

    let tool = GrepSearch::with_safety_config(permissive_safety_config());
    let args = serde_json::json!({
        "pattern": "match",
        "path": temp_dir.path().to_str().unwrap(),
        "max_matches": 2,
        "offset": 0
    });

    let result = tool.execute(args).await.unwrap();
    let matches = result["matches"].as_array().unwrap();
    assert_eq!(matches.len(), 2);
    assert_eq!(result["total_matches"], 5);
}

/// The search must honor the workspace path policy: `/etc/passwd` is outside
/// the workspace and must be rejected with the policy error before any file
/// is opened (regression for the search-tools bypass of path validation).
#[tokio::test]
async fn test_grep_search_rejects_out_of_workspace_path() {
    let tool = GrepSearch::with_safety_config(SafetyConfig::default());
    let args = serde_json::json!({
        "pattern": "root",
        "path": "/etc/passwd"
    });

    let result = tool.execute(args).await;
    let err = result.expect_err("grep_search on /etc/passwd must be rejected");
    let message = err.to_string();
    assert!(
        message.contains("outside working directory")
            || message.contains("not in allowed list")
            || message.contains("protected system path"),
        "expected a path-policy rejection, got: {}",
        message
    );
}

/// A `..`-based escape from a legal search root must be rejected as path
/// traversal.
#[tokio::test]
async fn test_grep_search_rejects_parent_escape() {
    let temp_dir = TempDir::new().unwrap();
    let escape_path = format!("{}/../..", temp_dir.path().to_str().unwrap());

    let tool = GrepSearch::with_safety_config(SafetyConfig::default());
    let args = serde_json::json!({
        "pattern": "root",
        "path": escape_path
    });

    let result = tool.execute(args).await;
    let err = result.expect_err("grep_search with .. escape must be rejected");
    let message = err.to_string();
    assert!(
        message.contains("outside working directory")
            || message.contains("traversal")
            || message.contains("not in allowed list"),
        "expected a path-policy rejection, got: {}",
        message
    );
}

/// A search root that is a symlink pointing outside the allowed tree must be
/// rejected (the validator resolves symlinks before the allow-list check).
#[cfg(unix)]
#[tokio::test]
async fn test_grep_search_rejects_symlink_escape() {
    use std::os::unix::fs::symlink;

    let outside = TempDir::new().unwrap();
    let allowed_root = TempDir::new().unwrap();
    let escape_link = allowed_root.path().join("escape");
    symlink(outside.path(), &escape_link).unwrap();

    let tool = GrepSearch::with_safety_config(restricted_safety_config(allowed_root.path()));
    let args = serde_json::json!({
        "pattern": "root",
        "path": escape_link.to_str().unwrap()
    });

    let result = tool.execute(args).await;
    let err = result.expect_err("grep_search through an escaping symlink must be rejected");
    let message = err.to_string();
    assert!(
        message.contains("not in allowed list") || message.contains("outside working directory"),
        "expected a path-policy rejection, got: {}",
        message
    );
}

/// A symlink that stays inside the allowed tree must still work.
#[cfg(unix)]
#[tokio::test]
async fn test_grep_search_allows_internal_symlink() {
    use std::os::unix::fs::symlink;

    let root = TempDir::new().unwrap();
    let real_dir = root.path().join("real");
    fs::create_dir_all(&real_dir).unwrap();
    fs::write(real_dir.join("test.txt"), "needle inside\n").unwrap();
    let link = root.path().join("link");
    symlink(&real_dir, &link).unwrap();

    let tool = GrepSearch::with_safety_config(restricted_safety_config(root.path()));
    let args = serde_json::json!({
        "pattern": "needle",
        "path": link.to_str().unwrap()
    });

    let result = tool.execute(args).await.unwrap();
    let matches = result["matches"].as_array().unwrap();
    assert_eq!(matches.len(), 1);
}

#[test]
fn test_standalone_grep_search() {
    let temp_dir = TempDir::new().unwrap();
    let file_path = temp_dir.path().join("test.txt");
    fs::write(&file_path, "hello world\nfoo bar\nhello again").unwrap();

    let result = grep_search("hello", temp_dir.path().to_str().unwrap(), true, 100, 0);
    assert_eq!(result.matches.len(), 2);
    assert_eq!(result.total_matches, 2);
}

/// P1 regression: a denied subdirectory inside an allowed search root must
/// not leak matches. The root passes the path policy, but the per-file policy
/// is applied to every discovered result (same rule `file_read` enforces), so
/// matches under `denied_dir` are dropped in both the ripgrep and the
/// built-in backend.
#[tokio::test]
async fn test_grep_search_denied_child_inside_allowed_root_yields_no_results() {
    let dir = TempDir::new().unwrap();
    fs::create_dir_all(dir.path().join("denied_dir")).unwrap();
    fs::write(dir.path().join("public.txt"), "needle in the open\n").unwrap();
    fs::write(
        dir.path().join("denied_dir/secret.txt"),
        "needle in a denied dir\n",
    )
    .unwrap();

    let tool = GrepSearch::with_safety_config(SafetyConfig {
        allowed_paths: vec!["/**".to_string()],
        denied_paths: vec!["denied_dir".to_string()],
        ..SafetyConfig::default()
    });
    let result = tool
        .execute(serde_json::json!({
            "pattern": "needle",
            "path": dir.path().to_str().unwrap()
        }))
        .await
        .unwrap();

    let matches = result["matches"].as_array().unwrap();
    assert!(!matches.is_empty(), "the allowed file must still match");
    for m in matches {
        let file = m["file"].as_str().unwrap();
        assert!(
            !file.contains("denied_dir"),
            "match from a denied subdirectory leaked: {}",
            file
        );
    }
    let leaked = matches
        .iter()
        .any(|m| m["file"].as_str().unwrap().contains("public.txt"));
    assert!(leaked, "the allowed file's match must be present");
}

/// P1 regression: a root that is itself denied is still rejected outright
/// (pre-existing root behavior preserved).
#[tokio::test]
async fn test_grep_search_denied_root_still_rejected() {
    let holder = TempDir::new().unwrap();
    let denied_root = holder.path().join("denied_root");
    fs::create_dir_all(&denied_root).unwrap();
    fs::write(denied_root.join("secret.txt"), "needle\n").unwrap();

    let tool = GrepSearch::with_safety_config(SafetyConfig {
        allowed_paths: vec!["/**".to_string()],
        denied_paths: vec!["denied_root".to_string()],
        ..SafetyConfig::default()
    });
    let result = tool
        .execute(serde_json::json!({
            "pattern": "needle",
            "path": denied_root.to_str().unwrap()
        }))
        .await;
    let err = result.expect_err("searching a denied root must be rejected");
    assert!(
        err.to_string().contains("denied"),
        "expected a denied-pattern rejection, got: {}",
        err
    );
}
