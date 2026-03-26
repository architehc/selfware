//! Tests for safety checker

use super::*;
use crate::api::types::{ToolCall, ToolFunction};
use crate::config::SafetyConfig;
use crate::safety::checker::validation::{is_private_or_internal, normalize_shell_command, split_shell_commands};

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
fn test_safety_checker_new() {
    let config = SafetyConfig::default();
    let checker = SafetyChecker::new(&config);
    assert_eq!(checker.config.allowed_paths, config.allowed_paths);
}

#[test]
fn test_safety_allows_safe_command() {
    let config = SafetyConfig::default();
    let checker = SafetyChecker::new(&config);

    let call = create_test_call("shell_exec", r#"{"command": "ls -la"}"#);
    assert!(checker.check_tool_call(&call).is_ok());
}

#[test]
fn test_safety_blocks_rm_rf_root() {
    let config = SafetyConfig::default();
    let checker = SafetyChecker::new(&config);

    let call = create_test_call("shell_exec", r#"{"command": "rm -rf /"}"#);
    let result = checker.check_tool_call(&call);
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("Dangerous command blocked"));
}

#[test]
fn test_safety_blocks_mkfs() {
    let config = SafetyConfig::default();
    let checker = SafetyChecker::new(&config);

    let call = create_test_call("shell_exec", r#"{"command": "mkfs.ext4 /dev/sda1"}"#);
    assert!(checker.check_tool_call(&call).is_err());
}

#[test]
fn test_safety_blocks_dd() {
    let config = SafetyConfig::default();
    let checker = SafetyChecker::new(&config);

    let call = create_test_call(
        "shell_exec",
        r#"{"command": "dd if=/dev/zero of=/dev/sda"}"#,
    );
    assert!(checker.check_tool_call(&call).is_err());
}

#[test]
fn test_safety_blocks_fork_bomb() {
    let config = SafetyConfig::default();
    let checker = SafetyChecker::new(&config);

    let call = create_test_call("shell_exec", r#"{"command": ":(){ :|:& };:"}"#);
    assert!(checker.check_tool_call(&call).is_err());
}

#[test]
fn test_safety_blocks_unknown_tool() {
    let config = SafetyConfig::default();
    let checker = SafetyChecker::new(&config);

    let call = create_test_call("unknown_tool", r#"{"arg": "value"}"#);
    let result = checker.check_tool_call(&call);
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("Unregistered tool"));
}

#[test]
fn test_normalize_shell_command_collapses_spaces() {
    let normalized = normalize_shell_command("rm   -rf    /");
    assert_eq!(normalized, "rm -rf /");
}

#[test]
fn test_normalize_shell_command_normalizes_slashes() {
    let normalized = normalize_shell_command("rm -rf //");
    assert_eq!(normalized, "rm -rf /");
}

#[test]
fn test_split_shell_commands_semicolon() {
    let parts = split_shell_commands("echo hello; rm -rf /");
    assert_eq!(parts.len(), 2);
    assert!(parts[0].contains("echo"));
    assert!(parts[1].contains("rm"));
}

#[test]
fn test_is_private_or_internal_loopback() {
    let ip: std::net::IpAddr = "127.0.0.1".parse().unwrap();
    assert!(is_private_or_internal(ip));
}

#[test]
fn test_is_private_or_internal_private_range() {
    let ip: std::net::IpAddr = "192.168.1.1".parse().unwrap();
    assert!(is_private_or_internal(ip));
}

#[test]
fn test_is_private_or_internal_public() {
    let ip: std::net::IpAddr = "8.8.8.8".parse().unwrap();
    assert!(!is_private_or_internal(ip));
}
