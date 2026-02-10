//! Package Manager Tools
//!
//! Tools for managing packages across different ecosystems:
//! - npm (Node.js)
//! - pip (Python)
//! - yarn (alternative Node.js)

use anyhow::{Context, Result};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::Path;
use std::process::Stdio;
use tokio::process::Command;

use super::Tool;

// ============================================================================
// NPM Tools
// ============================================================================

/// Install npm packages
pub struct NpmInstall;

#[async_trait]
impl Tool for NpmInstall {
    fn name(&self) -> &str {
        "npm_install"
    }

    fn description(&self) -> &str {
        "Install npm packages. Can install specific packages or all dependencies from package.json"
    }

    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "packages": {
                    "type": "array",
                    "items": {"type": "string"},
                    "description": "Package names to install (e.g., ['express', 'lodash@4.17.21']). If empty, installs from package.json"
                },
                "path": {
                    "type": "string",
                    "description": "Working directory (default: current directory)"
                },
                "dev": {
                    "type": "boolean",
                    "description": "Install as dev dependency (--save-dev)"
                },
                "global": {
                    "type": "boolean",
                    "description": "Install globally (-g)"
                }
            }
        })
    }

    async fn execute(&self, args: Value) -> Result<Value> {
        let packages: Vec<String> = args
            .get("packages")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();

        let path = args.get("path").and_then(|v| v.as_str()).unwrap_or(".");

        let dev = args.get("dev").and_then(|v| v.as_bool()).unwrap_or(false);
        let global = args
            .get("global")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let mut cmd = Command::new("npm");
        cmd.arg("install");

        if !packages.is_empty() {
            cmd.args(&packages);
        }

        if dev {
            cmd.arg("--save-dev");
        }

        if global {
            cmd.arg("-g");
        }

        cmd.current_dir(path);
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        let output = cmd.output().await.context("Failed to run npm install")?;

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        // Parse npm output for installed packages
        let installed = parse_npm_install_output(&stdout, &stderr);

        Ok(json!({
            "success": output.status.success(),
            "packages": if packages.is_empty() { "all from package.json".to_string() } else { packages.join(", ") },
            "installed": installed,
            "stdout": truncate_output(&stdout, 2000),
            "stderr": truncate_output(&stderr, 1000),
            "exit_code": output.status.code()
        }))
    }
}

/// Run npm scripts
pub struct NpmRun;

#[async_trait]
impl Tool for NpmRun {
    fn name(&self) -> &str {
        "npm_run"
    }

    fn description(&self) -> &str {
        "Run an npm script defined in package.json (e.g., 'npm run build', 'npm run dev')"
    }

    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "script": {
                    "type": "string",
                    "description": "Script name to run (e.g., 'build', 'dev', 'start')"
                },
                "path": {
                    "type": "string",
                    "description": "Working directory (default: current directory)"
                },
                "args": {
                    "type": "array",
                    "items": {"type": "string"},
                    "description": "Additional arguments to pass to the script"
                },
                "timeout_secs": {
                    "type": "integer",
                    "description": "Timeout in seconds (default: 300)"
                }
            },
            "required": ["script"]
        })
    }

    async fn execute(&self, args: Value) -> Result<Value> {
        let script = args
            .get("script")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("script is required"))?;

        let path = args.get("path").and_then(|v| v.as_str()).unwrap_or(".");

        let extra_args: Vec<String> = args
            .get("args")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();

        let timeout_secs = args
            .get("timeout_secs")
            .and_then(|v| v.as_u64())
            .unwrap_or(300);

        let mut cmd = Command::new("npm");
        cmd.arg("run");
        cmd.arg(script);

        if !extra_args.is_empty() {
            cmd.arg("--");
            cmd.args(&extra_args);
        }

        cmd.current_dir(path);
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        let output =
            tokio::time::timeout(std::time::Duration::from_secs(timeout_secs), cmd.output())
                .await
                .context("npm run timed out")?
                .context("Failed to run npm script")?;

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        Ok(json!({
            "success": output.status.success(),
            "script": script,
            "stdout": truncate_output(&stdout, 3000),
            "stderr": truncate_output(&stderr, 1000),
            "exit_code": output.status.code()
        }))
    }
}

/// List available npm scripts
pub struct NpmScripts;

#[async_trait]
impl Tool for NpmScripts {
    fn name(&self) -> &str {
        "npm_scripts"
    }

    fn description(&self) -> &str {
        "List available npm scripts from package.json"
    }

    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Path to directory containing package.json (default: current directory)"
                }
            }
        })
    }

    async fn execute(&self, args: Value) -> Result<Value> {
        let path = args.get("path").and_then(|v| v.as_str()).unwrap_or(".");

        let package_json_path = Path::new(path).join("package.json");

        if !package_json_path.exists() {
            return Ok(json!({
                "success": false,
                "error": "package.json not found",
                "path": package_json_path.display().to_string()
            }));
        }

        let content = tokio::fs::read_to_string(&package_json_path)
            .await
            .context("Failed to read package.json")?;

        let package: Value =
            serde_json::from_str(&content).context("Failed to parse package.json")?;

        let scripts: HashMap<String, String> = package
            .get("scripts")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default();

        let name = package
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");

        let version = package
            .get("version")
            .and_then(|v| v.as_str())
            .unwrap_or("0.0.0");

        Ok(json!({
            "success": true,
            "package": name,
            "version": version,
            "scripts": scripts,
            "count": scripts.len()
        }))
    }
}

// ============================================================================
// Pip Tools
// ============================================================================

/// Install Python packages with pip
pub struct PipInstall;

#[async_trait]
impl Tool for PipInstall {
    fn name(&self) -> &str {
        "pip_install"
    }

    fn description(&self) -> &str {
        "Install Python packages using pip"
    }

    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "packages": {
                    "type": "array",
                    "items": {"type": "string"},
                    "description": "Package names to install (e.g., ['requests', 'flask==2.0.0'])"
                },
                "requirements": {
                    "type": "string",
                    "description": "Path to requirements.txt file"
                },
                "upgrade": {
                    "type": "boolean",
                    "description": "Upgrade packages to latest version (--upgrade)"
                },
                "user": {
                    "type": "boolean",
                    "description": "Install to user site-packages (--user)"
                }
            }
        })
    }

    async fn execute(&self, args: Value) -> Result<Value> {
        let packages: Vec<String> = args
            .get("packages")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();

        let requirements = args.get("requirements").and_then(|v| v.as_str());
        let upgrade = args
            .get("upgrade")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let user = args.get("user").and_then(|v| v.as_bool()).unwrap_or(false);

        if packages.is_empty() && requirements.is_none() {
            return Ok(json!({
                "success": false,
                "error": "Either 'packages' or 'requirements' must be specified"
            }));
        }

        // Try python3 first, then python
        let python = find_python().await;

        let mut cmd = Command::new(&python);
        cmd.args(["-m", "pip", "install"]);

        if let Some(req_file) = requirements {
            cmd.args(["-r", req_file]);
        } else {
            cmd.args(&packages);
        }

        if upgrade {
            cmd.arg("--upgrade");
        }

        if user {
            cmd.arg("--user");
        }

        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        let output = cmd.output().await.context("Failed to run pip install")?;

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        let installed = parse_pip_install_output(&stdout);

        Ok(json!({
            "success": output.status.success(),
            "python": python,
            "packages": if let Some(req) = requirements {
                format!("from {}", req)
            } else {
                packages.join(", ")
            },
            "installed": installed,
            "stdout": truncate_output(&stdout, 2000),
            "stderr": truncate_output(&stderr, 1000),
            "exit_code": output.status.code()
        }))
    }
}

/// List installed Python packages
pub struct PipList;

#[async_trait]
impl Tool for PipList {
    fn name(&self) -> &str {
        "pip_list"
    }

    fn description(&self) -> &str {
        "List installed Python packages"
    }

    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "outdated": {
                    "type": "boolean",
                    "description": "Show only outdated packages"
                },
                "format": {
                    "type": "string",
                    "enum": ["columns", "json"],
                    "description": "Output format (default: json)"
                }
            }
        })
    }

    async fn execute(&self, args: Value) -> Result<Value> {
        let outdated = args
            .get("outdated")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let python = find_python().await;

        let mut cmd = Command::new(&python);
        cmd.args(["-m", "pip", "list", "--format=json"]);

        if outdated {
            cmd.arg("--outdated");
        }

        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        let output = cmd.output().await.context("Failed to run pip list")?;

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        let packages: Vec<PipPackage> = serde_json::from_str(&stdout).unwrap_or_default();

        Ok(json!({
            "success": output.status.success(),
            "python": python,
            "packages": packages,
            "count": packages.len(),
            "outdated_only": outdated,
            "stderr": if stderr.is_empty() { None } else { Some(truncate_output(&stderr, 500)) }
        }))
    }
}

/// Freeze pip packages to requirements.txt format
pub struct PipFreeze;

#[async_trait]
impl Tool for PipFreeze {
    fn name(&self) -> &str {
        "pip_freeze"
    }

    fn description(&self) -> &str {
        "Output installed packages in requirements.txt format"
    }

    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "output_file": {
                    "type": "string",
                    "description": "Write output to file (e.g., 'requirements.txt')"
                }
            }
        })
    }

    async fn execute(&self, args: Value) -> Result<Value> {
        let output_file = args.get("output_file").and_then(|v| v.as_str());

        let python = find_python().await;

        let mut cmd = Command::new(&python);
        cmd.args(["-m", "pip", "freeze"]);
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        let output = cmd.output().await.context("Failed to run pip freeze")?;

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        let packages: Vec<&str> = stdout.lines().filter(|l| !l.is_empty()).collect();

        if let Some(file_path) = output_file {
            tokio::fs::write(file_path, &stdout)
                .await
                .context("Failed to write requirements file")?;
        }

        Ok(json!({
            "success": output.status.success(),
            "python": python,
            "requirements": stdout.trim(),
            "count": packages.len(),
            "written_to": output_file,
            "stderr": if stderr.is_empty() { None } else { Some(truncate_output(&stderr, 500)) }
        }))
    }
}

// ============================================================================
// Yarn Tools (Alternative to npm)
// ============================================================================

/// Install packages with Yarn
pub struct YarnInstall;

#[async_trait]
impl Tool for YarnInstall {
    fn name(&self) -> &str {
        "yarn_install"
    }

    fn description(&self) -> &str {
        "Install packages using Yarn (alternative to npm)"
    }

    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "packages": {
                    "type": "array",
                    "items": {"type": "string"},
                    "description": "Package names to install. If empty, installs from package.json"
                },
                "path": {
                    "type": "string",
                    "description": "Working directory (default: current directory)"
                },
                "dev": {
                    "type": "boolean",
                    "description": "Install as dev dependency (--dev)"
                }
            }
        })
    }

    async fn execute(&self, args: Value) -> Result<Value> {
        let packages: Vec<String> = args
            .get("packages")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();

        let path = args.get("path").and_then(|v| v.as_str()).unwrap_or(".");

        let dev = args.get("dev").and_then(|v| v.as_bool()).unwrap_or(false);

        let mut cmd = Command::new("yarn");

        if packages.is_empty() {
            cmd.arg("install");
        } else {
            cmd.arg("add");
            cmd.args(&packages);
            if dev {
                cmd.arg("--dev");
            }
        }

        cmd.current_dir(path);
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        let output = cmd.output().await.context("Failed to run yarn")?;

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        Ok(json!({
            "success": output.status.success(),
            "packages": if packages.is_empty() { "all from package.json".to_string() } else { packages.join(", ") },
            "stdout": truncate_output(&stdout, 2000),
            "stderr": truncate_output(&stderr, 1000),
            "exit_code": output.status.code()
        }))
    }
}

// ============================================================================
// Helper Functions
// ============================================================================

#[derive(Debug, Serialize, Deserialize)]
struct PipPackage {
    name: String,
    version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    latest_version: Option<String>,
}

/// Find the Python executable (python3 or python)
async fn find_python() -> String {
    // Try python3 first
    if Command::new("python3")
        .arg("--version")
        .output()
        .await
        .map(|o| o.status.success())
        .unwrap_or(false)
    {
        return "python3".to_string();
    }

    // Fall back to python
    "python".to_string()
}

/// Parse npm install output for installed packages
fn parse_npm_install_output(stdout: &str, stderr: &str) -> Vec<String> {
    let mut installed = Vec::new();
    let combined = format!("{}\n{}", stdout, stderr);

    for line in combined.lines() {
        // Look for "added X packages" pattern
        if line.contains("added") && line.contains("package") {
            installed.push(line.trim().to_string());
        }
        // Look for "+ package@version" pattern
        if line.starts_with("+ ") || line.starts_with("added ") {
            installed.push(line.trim().to_string());
        }
    }

    installed
}

/// Parse pip install output for installed packages
fn parse_pip_install_output(stdout: &str) -> Vec<String> {
    let mut installed = Vec::new();

    for line in stdout.lines() {
        // Look for "Successfully installed" line
        if line.starts_with("Successfully installed") {
            let packages = line
                .strip_prefix("Successfully installed ")
                .unwrap_or("")
                .split_whitespace()
                .map(String::from)
                .collect::<Vec<_>>();
            installed.extend(packages);
        }
        // Look for "Requirement already satisfied" for existing packages
        if line.starts_with("Requirement already satisfied:") {
            if let Some(pkg) = line.split(':').nth(1) {
                if let Some(name) = pkg.split_whitespace().next() {
                    installed.push(format!("{} (already installed)", name));
                }
            }
        }
    }

    installed
}

/// Truncate output to max length with indicator
fn truncate_output(output: &str, max_len: usize) -> String {
    if output.len() <= max_len {
        output.to_string()
    } else {
        format!(
            "{}... [truncated, {} total chars]",
            &output[..max_len],
            output.len()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_npm_install_schema() {
        let tool = NpmInstall;
        let schema = tool.schema();
        assert!(schema.get("properties").is_some());
        assert!(schema["properties"].get("packages").is_some());
    }

    #[test]
    fn test_npm_run_schema() {
        let tool = NpmRun;
        let schema = tool.schema();
        assert!(schema.get("required").is_some());
        assert!(schema["required"]
            .as_array()
            .unwrap()
            .contains(&json!("script")));
    }

    #[test]
    fn test_pip_install_schema() {
        let tool = PipInstall;
        let schema = tool.schema();
        assert!(schema["properties"].get("packages").is_some());
        assert!(schema["properties"].get("requirements").is_some());
    }

    #[test]
    fn test_parse_npm_install_output() {
        let stdout = "added 5 packages in 2s";
        let stderr = "";
        let result = parse_npm_install_output(stdout, stderr);
        assert!(!result.is_empty());
        assert!(result[0].contains("added"));
    }

    #[test]
    fn test_parse_npm_install_output_with_plus() {
        let stdout = "+ express@4.18.2\n+ lodash@4.17.21";
        let stderr = "";
        let result = parse_npm_install_output(stdout, stderr);
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_parse_pip_install_output() {
        let stdout = "Collecting requests\nSuccessfully installed requests-2.28.0 urllib3-1.26.0";
        let result = parse_pip_install_output(stdout);
        assert_eq!(result.len(), 2);
        assert!(result.contains(&"requests-2.28.0".to_string()));
    }

    #[test]
    fn test_parse_pip_install_already_satisfied() {
        let stdout = "Requirement already satisfied: requests in /usr/lib/python3/dist-packages";
        let result = parse_pip_install_output(stdout);
        assert_eq!(result.len(), 1);
        assert!(result[0].contains("already installed"));
    }

    #[test]
    fn test_truncate_output_short() {
        let output = "short output";
        assert_eq!(truncate_output(output, 100), output);
    }

    #[test]
    fn test_truncate_output_long() {
        let output = "a".repeat(200);
        let result = truncate_output(&output, 50);
        assert!(result.contains("truncated"));
        assert!(result.contains("200 total chars"));
    }

    #[test]
    fn test_tool_names() {
        assert_eq!(NpmInstall.name(), "npm_install");
        assert_eq!(NpmRun.name(), "npm_run");
        assert_eq!(NpmScripts.name(), "npm_scripts");
        assert_eq!(PipInstall.name(), "pip_install");
        assert_eq!(PipList.name(), "pip_list");
        assert_eq!(PipFreeze.name(), "pip_freeze");
        assert_eq!(YarnInstall.name(), "yarn_install");
    }

    #[test]
    fn test_tool_descriptions() {
        assert!(!NpmInstall.description().is_empty());
        assert!(!NpmRun.description().is_empty());
        assert!(!PipInstall.description().is_empty());
        assert!(PipInstall.description().contains("pip"));
    }

    #[tokio::test]
    async fn test_npm_scripts_no_package_json() {
        let tool = NpmScripts;
        let result = tool
            .execute(json!({"path": "/nonexistent/path"}))
            .await
            .unwrap();
        assert_eq!(result["success"], false);
        assert!(result["error"].as_str().unwrap().contains("not found"));
    }

    #[tokio::test]
    async fn test_pip_install_no_packages() {
        let tool = PipInstall;
        let result = tool.execute(json!({})).await.unwrap();
        assert_eq!(result["success"], false);
        assert!(result["error"]
            .as_str()
            .unwrap()
            .contains("must be specified"));
    }
}
