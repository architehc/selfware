//! E2E test for all tools on a test project

use selfware::tools::ToolRegistry;
use std::fs;
use tempfile::tempdir;

#[tokio::test]
async fn test_e2e_file_tools() {
    let cfg = selfware::config::SafetyConfig {
        allowed_paths: vec!["/**".to_string()],
        ..Default::default()
    };
    selfware::tools::file::init_safety_config(&cfg);
    let dir = tempdir().unwrap();
    let test_file = dir.path().join("test.rs");

    // Create test file
    fs::write(
        &test_file,
        r#"
fn hello() -> &'static str {
    "Hello, World!"
}

fn main() {
    println!("{}", hello());
}
"#,
    )
    .unwrap();

    let registry = ToolRegistry::new();

    // Test FileRead
    let file_read = registry.get("file_read").unwrap();
    let result = file_read
        .execute(serde_json::json!({
            "path": test_file.to_str().unwrap()
        }))
        .await
        .unwrap();
    assert!(result["content"]
        .as_str()
        .unwrap()
        .contains("Hello, World!"));
    println!("✓ FileRead works");

    // Test FileEdit
    let file_edit = registry.get("file_edit").unwrap();
    let result = file_edit
        .execute(serde_json::json!({
            "path": test_file.to_str().unwrap(),
            "old_str": "Hello, World!",
            "new_str": "Hello, Rust!"
        }))
        .await
        .unwrap();
    assert_eq!(result["success"], true);
    println!("✓ FileEdit works");

    // Verify edit
    let content = fs::read_to_string(&test_file).unwrap();
    assert!(content.contains("Hello, Rust!"));
    println!("✓ FileEdit verified");

    // Test DirectoryTree
    let dir_tree = registry.get("directory_tree").unwrap();
    let result = dir_tree
        .execute(serde_json::json!({
            "path": dir.path().to_str().unwrap()
        }))
        .await
        .unwrap();
    assert!(result["total"].as_i64().unwrap() >= 1);
    println!("✓ DirectoryTree works");
}

#[tokio::test]
async fn test_e2e_search_tools() {
    let dir = tempdir().unwrap();

    // Create test files
    fs::write(
        dir.path().join("main.rs"),
        r#"
fn calculate_sum(a: i32, b: i32) -> i32 {
    a + b
}

struct Calculator {
    value: i32,
}

fn main() {
    let result = calculate_sum(1, 2);
}
"#,
    )
    .unwrap();

    fs::write(
        dir.path().join("lib.rs"),
        r#"
pub fn helper_function() -> bool {
    true
}
"#,
    )
    .unwrap();

    let registry = ToolRegistry::new();

    // Test GrepSearch
    let grep = registry.get("grep_search").unwrap();
    let result = grep
        .execute(serde_json::json!({
            "pattern": "calculate",
            "path": dir.path().to_str().unwrap()
        }))
        .await
        .unwrap();
    assert!(result["count"].as_i64().unwrap() >= 1);
    println!("✓ GrepSearch works - found {} matches", result["count"]);

    // Test GlobFind
    let glob = registry.get("glob_find").unwrap();
    let result = glob
        .execute(serde_json::json!({
            "pattern": "*.rs",
            "path": dir.path().to_str().unwrap()
        }))
        .await
        .unwrap();
    assert_eq!(result["count"], 2);
    println!("✓ GlobFind works - found {} files", result["count"]);

    // Test SymbolSearch
    let symbol = registry.get("symbol_search").unwrap();
    let result = symbol
        .execute(serde_json::json!({
            "name": "calculate",
            "path": dir.path().to_str().unwrap(),
            "symbol_type": "function"
        }))
        .await
        .unwrap();
    assert!(!result["symbols"].as_array().unwrap().is_empty());
    println!(
        "✓ SymbolSearch works - found {} symbols",
        result["symbols"].as_array().unwrap().len()
    );
}

#[tokio::test]
async fn test_e2e_cargo_tools() {
    // Use our actual project for cargo tools
    let registry = ToolRegistry::new();

    // Test CargoCheck (just schema, actual run needs cargo project)
    let cargo_check = registry.get("cargo_check").unwrap();
    assert_eq!(cargo_check.name(), "cargo_check");
    println!("✓ CargoCheck registered");

    // Test CargoTest (just schema)
    let cargo_test = registry.get("cargo_test").unwrap();
    assert_eq!(cargo_test.name(), "cargo_test");
    println!("✓ CargoTest registered");

    // Test CargoClippy (just schema)
    let cargo_clippy = registry.get("cargo_clippy").unwrap();
    assert_eq!(cargo_clippy.name(), "cargo_clippy");
    println!("✓ CargoClippy registered");
}

#[tokio::test]
async fn test_e2e_shell_tool() {
    let registry = ToolRegistry::new();

    let shell = registry.get("shell_exec").unwrap();
    let result = shell
        .execute(serde_json::json!({
            "command": "echo 'E2E test successful'",
            "timeout_secs": 5
        }))
        .await
        .unwrap();

    assert_eq!(result["exit_code"], 0);
    assert!(result["stdout"]
        .as_str()
        .unwrap()
        .contains("E2E test successful"));
    println!("✓ ShellExec works");
}

#[tokio::test]
async fn test_e2e_all_tools_registered() {
    let registry = ToolRegistry::new();
    let tools = registry.list();
    let tool_names: Vec<&str> = tools.iter().map(|t| t.name()).collect();

    // Expected tools
    let expected = vec![
        "file_read",
        "file_write",
        "file_edit",
        "directory_tree",
        "git_status",
        "git_diff",
        "git_commit",
        "git_checkpoint",
        "cargo_check",
        "cargo_test",
        "cargo_clippy",
        "shell_exec",
        "grep_search",
        "glob_find",
        "symbol_search",
        "http_request",
    ];

    for tool in &expected {
        assert!(tool_names.contains(tool), "Missing tool: {}", tool);
    }

    println!(
        "✓ All {} tools registered: {:?}",
        tool_names.len(),
        tool_names
    );
}
