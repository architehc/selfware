use super::Tool;
use anyhow::{Context, Result};
use async_trait::async_trait;
use serde::Deserialize;
use serde_json::Value;

pub struct CargoTest;
pub struct CargoCheck;
pub struct CargoClippy;
pub struct CargoFmt;

#[async_trait]
impl Tool for CargoTest {
    fn name(&self) -> &str { "cargo_test" }
    
    fn description(&self) -> &str {
        "Run cargo test to verify changes. Critical for self-modification. Returns test results and failure count."
    }
    
    fn schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "package": {"type": "string", "description": "Specific package to test"},
                "test_name": {"type": "string", "description": "Specific test to run (substring match)"},
                "release": {"type": "boolean", "default": false, "description": "Run tests in release mode"},
                "no_fail_fast": {"type": "boolean", "default": true, "description": "Run all tests even if some fail"}
            }
        })
    }
    
    async fn execute(&self, args: Value) -> Result<Value> {
        let mut cmd = tokio::process::Command::new("cargo");
        cmd.arg("test");
        
        if let Some(pkg) = args.get("package").and_then(|v| v.as_str()) {
            cmd.arg("-p").arg(pkg);
        }
        
        if let Some(name) = args.get("test_name").and_then(|v| v.as_str()) {
            cmd.arg(name);
        }
        
        if args.get("release").and_then(|v| v.as_bool()).unwrap_or(false) {
            cmd.arg("--release");
        }
        
        if args.get("no_fail_fast").and_then(|v| v.as_bool()).unwrap_or(true) {
            cmd.arg("--no-fail-fast");
        }
        
        cmd.arg("--message-format=short");
        cmd.env("RUST_BACKTRACE", "1");
        
        let output = cmd.output().await.context("Failed to execute cargo test")?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        
        // Parse test results
        let passed = stdout.lines().filter(|l| l.contains("test result: ok")).count();
        let failed = stdout.lines().filter(|l| l.contains("test result: FAILED")).count();
        let total = stdout.matches("test ").count();
        
        Ok(serde_json::json!({
            "success": output.status.success() && failed == 0,
            "tests_passed": passed,
            "tests_failed": failed,
            "total_tests": total,
            "stdout": stdout.chars().take(8000).collect::<String>(),
            "stderr": stderr.chars().take(4000).collect::<String>(),
            "exit_code": output.status.code()
        }))
    }
}

#[async_trait]
impl Tool for CargoCheck {
    fn name(&self) -> &str { "cargo_check" }
    
    fn description(&self) -> &str {
        "Run cargo check for fast compile verification. Use this before tests for quick feedback."
    }
    
    fn schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "all_targets": {"type": "boolean", "default": true, "description": "Check all targets including tests"},
                "all_features": {"type": "boolean", "default": true, "description": "Check with all features enabled"},
                "release": {"type": "boolean", "default": false}
            }
        })
    }
    
    async fn execute(&self, args: Value) -> Result<Value> {
        let mut cmd = tokio::process::Command::new("cargo");
        cmd.arg("check");
        
        if args.get("all_targets").and_then(|v| v.as_bool()).unwrap_or(true) {
            cmd.arg("--all-targets");
        }
        
        if args.get("all_features").and_then(|v| v.as_bool()).unwrap_or(true) {
            cmd.arg("--all-features");
        }
        
        if args.get("release").and_then(|v| v.as_bool()).unwrap_or(false) {
            cmd.arg("--release");
        }
        
        let output = cmd.output().await.context("Failed to execute cargo check")?;
        let stderr = String::from_utf8_lossy(&output.stderr);
        
        // Count errors and warnings
        let errors = stderr.lines().filter(|l| l.contains("error[")).count();
        let warnings = stderr.lines().filter(|l| l.contains("warning:")).count();
        
        Ok(serde_json::json!({
            "success": output.status.success(),
            "errors": errors,
            "warnings": warnings,
            "output": stderr.chars().take(6000).collect::<String>(),
            "exit_code": output.status.code()
        }))
    }
}

#[async_trait]
impl Tool for CargoClippy {
    fn name(&self) -> &str { "cargo_clippy" }
    
    fn description(&self) -> &str {
        "Run cargo clippy with strict linting configuration. Use to ensure code quality and catch common mistakes."
    }
    
    fn schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "all_targets": {"type": "boolean", "default": true},
                "fix": {"type": "boolean", "default": false, "description": "Automatically apply safe fixes"},
                "deny_warnings": {"type": "boolean", "default": true}
            }
        })
    }
    
    async fn execute(&self, args: Value) -> Result<Value> {
        let mut cmd = tokio::process::Command::new("cargo");
        cmd.arg("clippy");
        
        if args.get("all_targets").and_then(|v| v.as_bool()).unwrap_or(true) {
            cmd.arg("--all-targets");
        }
        
        if args.get("fix").and_then(|v| v.as_bool()).unwrap_or(false) {
            cmd.arg("--fix").arg("--allow-staged").arg("--allow-dirty");
        }
        
        if args.get("deny_warnings").and_then(|v| v.as_bool()).unwrap_or(true) {
            cmd.arg("--").arg("-D").arg("warnings");
        }
        
        cmd.args(&["--", "-D", "clippy::unwrap_used", "-D", "clippy::expect_used"]);
        
        let output = cmd.output().await.context("Failed to execute cargo clippy")?;
        let stderr = String::from_utf8_lossy(&output.stderr);
        
        let errors = stderr.lines().filter(|l| l.contains("error:")).count();
        let warnings = stderr.lines().filter(|l| l.contains("warning:")).count();
        
        Ok(serde_json::json!({
            "success": output.status.success(),
            "errors": errors,
            "warnings": warnings,
            "output": stderr.chars().take(6000).collect::<String>(),
            "fixed": args.get("fix").and_then(|v| v.as_bool()).unwrap_or(false)
        }))
    }
}

#[async_trait]
impl Tool for CargoFmt {
    fn name(&self) -> &str { "cargo_fmt" }
    
    fn description(&self) -> &str {
        "Run cargo fmt to format code. Use --check to verify formatting without changing."
    }
    
    fn schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "check": {"type": "boolean", "default": false, "description": "Check formatting without modifying"},
                "all": {"type": "boolean", "default": true, "description": "Format all targets"}
            }
        })
    }
    
    async fn execute(&self, args: Value) -> Result<Value> {
        let mut cmd = tokio::process::Command::new("cargo");
        cmd.arg("fmt");
        
        if args.get("all").and_then(|v| v.as_bool()).unwrap_or(true) {
            cmd.arg("--all");
        }
        
        if args.get("check").and_then(|v| v.as_bool()).unwrap_or(false) {
            cmd.arg("--").arg("--check");
        }
        
        let output = cmd.output().await.context("Failed to execute cargo fmt")?;
        
        Ok(serde_json::json!({
            "success": output.status.success(),
            "diff": String::from_utf8_lossy(&output.stderr).to_string(),
            "exit_code": output.status.code()
        }))
    }
}
