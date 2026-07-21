use super::*;
use std::fs;
use tempfile::TempDir;

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

    let candidates =
        localize_issue_sync("process_data returns wrong value", repo.to_str().unwrap()).unwrap();
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
    let candidates = localize_issue_sync("a", temp_dir.path().to_str().unwrap()).unwrap();
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

    let tool = LocalizeIssue;
    let args = serde_json::json!({
        "issue": "fix_me is broken",
        "repo_path": repo.to_str().unwrap()
    });

    let result = tool.execute(args).await.unwrap();
    let candidates = result["candidates"].as_array().unwrap();
    assert!(!candidates.is_empty());
    assert!(candidates[0]["file"].as_str().unwrap().contains("lib.rs"));
}
