use selfware::safety::SafetyChecker;
use selfware::config::SafetyConfig;
use selfware::api::types::{ToolCall, ToolFunction};

fn create_test_call(name: &str, args: &str) -> ToolCall {
    ToolCall {
        id: "test".to_string(),
        call_type: "function".to_string(),
        function: ToolFunction {
            name: name.to_string(),
            arguments: args.to_string(),
        },
    }
}

#[test]
fn test_safety_allows_safe_command() {
    let config = SafetyConfig::default();
    let checker = SafetyChecker::new(&config);
    
    let call = create_test_call("shell_exec", r#"{"command": "ls -la"}"#);
    assert!(checker.check_tool_call(&call).is_ok());
}

#[test]
fn test_safety_blocks_dangerous_command() {
    let config = SafetyConfig::default();
    let checker = SafetyChecker::new(&config);
    
    let call = create_test_call("shell_exec", r#"{"command": "rm -rf /"}"#);
    assert!(checker.check_tool_call(&call).is_err());
}

#[test]
fn test_safety_blocks_path_traversal() {
    let config = SafetyConfig {
        allowed_paths: vec!["/safe/**".to_string()],
        ..Default::default()
    };
    let checker = SafetyChecker::new(&config);
    
    let call = create_test_call("file_read", r#"{"path": "/etc/passwd"}"#);
    assert!(checker.check_tool_call(&call).is_err());
}
