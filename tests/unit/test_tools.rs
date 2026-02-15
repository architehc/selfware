use selfware::tools::{ToolRegistry, file::FileRead, shell::ShellExec};
use serde_json::json;

#[tokio::test]
async fn test_file_read_success() {
    let tool = FileRead;
    let args = json!({"path": "Cargo.toml"});
    
    let result = tool.execute(args).await.unwrap();
    assert!(result.get("content").is_some());
    assert_eq!(result.get("encoding").unwrap(), "utf-8");
}

#[tokio::test]
async fn test_file_read_not_found() {
    let tool = FileRead;
    let args = json!({"path": "/nonexistent/file.txt"});
    
    let result = tool.execute(args).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_shell_exec_echo() {
    let tool = ShellExec;
    let args = json!({"command": "echo 'hello'", "timeout_secs": 5});
    
    let result = tool.execute(args).await.unwrap();
    assert_eq!(result.get("exit_code").unwrap(), 0);
    assert!(result.get("stdout").unwrap().as_str().unwrap().contains("hello"));
}

#[tokio::test]
async fn test_tool_registry() {
    let registry = ToolRegistry::new();
    assert!(registry.get("file_read").is_some());
    assert!(registry.get("shell_exec").is_some());
    assert!(registry.get("nonexistent").is_none());
}
