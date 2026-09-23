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
use tokio::process::Command;

use super::Tool;
use crate::config::SafetyConfig;
use crate::safety::process_env::SanitizedEnvExt;
use crate::tools::file::{resolve_safety_config, validate_tool_path};

/// Maximum output buffer size from a package command (10 MB).
/// Prevents a runaway command from consuming unlimited memory while draining pipes.
const MAX_PACKAGE_OUTPUT_SIZE: usize = 10 * 1024 * 1024;

// ============================================================================
// NPM Tools
// ============================================================================

/// Install npm packages
#[derive(Default)]
pub struct NpmInstall {
    /// Per-instance safety config for path-policy enforcement; falls back to
    /// the process-global config when `None`.
    pub safety_config: Option<SafetyConfig>,
}

impl NpmInstall {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_safety_config(config: SafetyConfig) -> Self {
        Self {
            safety_config: Some(config),
        }
    }
}

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
                },
                "timeout_secs": {
                    "type": "integer",
                    "description": "Timeout in seconds (default: 300)"
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

        // The install MUTATES the working directory (writes node_modules,
        // package-lock.json) — the cwd must obey the workspace path policy
        // like any other write target (2026-09-21 review sweep).
        let safety = resolve_safety_config(self.safety_config.as_ref());
        validate_tool_path(path, &safety)?;

        let mut cmd = Command::new("npm");
        crate::safety::process_env::sanitize_command_env(&mut cmd);
        crate::tools::workspace_root::CommandRootExt::in_workspace_root(&mut cmd);
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

        cmd.current_dir(crate::tools::workspace_root::anchor(path));

        let timeout_secs = args
            .get("timeout_secs")
            .and_then(|v| v.as_u64())
            .unwrap_or(300);
        let output = crate::tools::process_guard::run_command_bounded(
            cmd,
            std::time::Duration::from_secs(timeout_secs),
            MAX_PACKAGE_OUTPUT_SIZE,
        )
        .await
        .map_err(|e| match e {
            crate::tools::process_guard::CommandRunError::Timeout(_) => {
                anyhow::anyhow!("npm install timed out (the npm process was killed)")
            }
            other => anyhow::anyhow!("Failed to run npm install: {}", other),
        })?;

        let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();

        // Parse npm output for installed packages
        let installed = parse_npm_install_output(&stdout, &stderr);

        Ok(json!({
            "success": output.success(),
            "packages": if packages.is_empty() { "all from package.json".to_string() } else { packages.join(", ") },
            "installed": installed,
            "stdout": truncate_output(&stdout, 2000),
            "stderr": truncate_output(&stderr, 1000),
            "exit_code": output.exit_code()
        }))
    }
}

/// Run npm scripts
#[derive(Default)]
pub struct NpmRun {
    /// Per-instance safety config for path-policy enforcement.
    pub safety_config: Option<SafetyConfig>,
}

impl NpmRun {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_safety_config(config: SafetyConfig) -> Self {
        Self {
            safety_config: Some(config),
        }
    }
}

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

        // The script runs with `path` as its working directory, and scripts
        // may mutate the tree — the cwd obeys the workspace path policy.
        let safety = resolve_safety_config(self.safety_config.as_ref());
        validate_tool_path(path, &safety)?;

        let mut cmd = Command::new("npm");
        crate::safety::process_env::sanitize_command_env(&mut cmd);
        crate::tools::workspace_root::CommandRootExt::in_workspace_root(&mut cmd);
        cmd.arg("run");
        cmd.arg(script);

        if !extra_args.is_empty() {
            cmd.arg("--");
            cmd.args(&extra_args);
        }

        cmd.current_dir(crate::tools::workspace_root::anchor(path));

        let output = crate::tools::process_guard::run_command_bounded(
            cmd,
            std::time::Duration::from_secs(timeout_secs),
            MAX_PACKAGE_OUTPUT_SIZE,
        )
        .await
        .map_err(|e| match e {
            crate::tools::process_guard::CommandRunError::Timeout(_) => {
                anyhow::anyhow!("npm run timed out (the npm process was killed)")
            }
            other => anyhow::anyhow!("Failed to run npm script: {}", other),
        })?;

        let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();

        Ok(json!({
            "success": output.success(),
            "script": script,
            "stdout": truncate_output(&stdout, 3000),
            "stderr": truncate_output(&stderr, 1000),
            "exit_code": output.exit_code()
        }))
    }
}

/// List available npm scripts
#[derive(Default)]
pub struct NpmScripts {
    /// Per-instance safety config for path-policy enforcement.
    pub safety_config: Option<SafetyConfig>,
}

impl NpmScripts {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_safety_config(config: SafetyConfig) -> Self {
        Self {
            safety_config: Some(config),
        }
    }
}

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

        // The tool READS package.json under `path` — the directory obeys the
        // same workspace path policy as file_read.
        let safety = resolve_safety_config(self.safety_config.as_ref());
        validate_tool_path(path, &safety)?;

        let package_json_path =
            Path::new(&crate::tools::workspace_root::anchor(path)).join("package.json");

        if !package_json_path.exists() {
            anyhow::bail!("package.json not found: {}", package_json_path.display());
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
#[derive(Default)]
pub struct PipInstall {
    /// Per-instance safety config for path-policy enforcement.
    pub safety_config: Option<SafetyConfig>,
}

impl PipInstall {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_safety_config(config: SafetyConfig) -> Self {
        Self {
            safety_config: Some(config),
        }
    }
}

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
                },
                "timeout_secs": {
                    "type": "integer",
                    "description": "Timeout in seconds (default: 300)"
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
            anyhow::bail!("Either 'packages' or 'requirements' must be specified");
        }

        // The `requirements` file is READ and pip installs packages globally
        // or into the active environment — validate the requirements path
        // against the workspace policy (2026-09-21 review sweep).
        if let Some(req_file) = requirements {
            if !req_file.is_empty() {
                let safety = resolve_safety_config(self.safety_config.as_ref());
                validate_tool_path(req_file, &safety)?;
            }
        }

        // Try python3 first, then python
        let python = find_python().await;

        let mut cmd = Command::new(&python);
        crate::safety::process_env::sanitize_command_env(&mut cmd);
        crate::tools::workspace_root::CommandRootExt::in_workspace_root(&mut cmd);
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

        let timeout_secs = args
            .get("timeout_secs")
            .and_then(|v| v.as_u64())
            .unwrap_or(300);
        let output = crate::tools::process_guard::run_command_bounded(
            cmd,
            std::time::Duration::from_secs(timeout_secs),
            MAX_PACKAGE_OUTPUT_SIZE,
        )
        .await
        .map_err(|e| match e {
            crate::tools::process_guard::CommandRunError::Timeout(_) => {
                anyhow::anyhow!("pip install timed out (the pip process was killed)")
            }
            other => anyhow::anyhow!("Failed to run pip install: {}", other),
        })?;

        let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();

        let installed = parse_pip_install_output(&stdout);

        Ok(json!({
            "success": output.success(),
            "python": python,
            "packages": if let Some(req) = requirements {
                format!("from {}", req)
            } else {
                packages.join(", ")
            },
            "installed": installed,
            "stdout": truncate_output(&stdout, 2000),
            "stderr": truncate_output(&stderr, 1000),
            "exit_code": output.exit_code()
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
        crate::safety::process_env::sanitize_command_env(&mut cmd);
        crate::tools::workspace_root::CommandRootExt::in_workspace_root(&mut cmd);
        cmd.args(["-m", "pip", "list", "--format=json"]);

        if outdated {
            cmd.arg("--outdated");
        }

        let output = crate::tools::process_guard::run_command_bounded(
            cmd,
            std::time::Duration::from_secs(60),
            MAX_PACKAGE_OUTPUT_SIZE,
        )
        .await
        .context("Failed to run pip list")?;

        let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();

        let packages: Vec<PipPackage> = serde_json::from_str(&stdout).unwrap_or_default();

        Ok(json!({
            "success": output.success(),
            "python": python,
            "packages": packages,
            "count": packages.len(),
            "outdated_only": outdated,
            "stderr": if stderr.is_empty() { None } else { Some(truncate_output(&stderr, 500)) }
        }))
    }
}

/// Freeze pip packages to requirements.txt format
#[derive(Default)]
pub struct PipFreeze {
    /// Per-instance safety config for path-policy enforcement.
    pub safety_config: Option<SafetyConfig>,
}

impl PipFreeze {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_safety_config(config: SafetyConfig) -> Self {
        Self {
            safety_config: Some(config),
        }
    }
}

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
        crate::safety::process_env::sanitize_command_env(&mut cmd);
        crate::tools::workspace_root::CommandRootExt::in_workspace_root(&mut cmd);
        cmd.args(["-m", "pip", "freeze"]);
        let output = crate::tools::process_guard::run_command_bounded(
            cmd,
            std::time::Duration::from_secs(60),
            MAX_PACKAGE_OUTPUT_SIZE,
        )
        .await
        .context("Failed to run pip freeze")?;

        let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();

        let packages: Vec<&str> = stdout.lines().filter(|l| !l.is_empty()).collect();

        if let Some(file_path) = output_file {
            // Arbitrary-write fix (2026-09-21 review): `output_file` was
            // forwarded to fs::write unvalidated, so `pip_freeze` could
            // overwrite any path the process could write. The write target
            // now obeys the same workspace path policy as file_write.
            let safety = resolve_safety_config(self.safety_config.as_ref());
            validate_tool_path(file_path, &safety)?;
            tokio::fs::write(file_path, &stdout)
                .await
                .context("Failed to write requirements file")?;
        }

        Ok(json!({
            "success": output.success(),
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
#[derive(Default)]
pub struct YarnInstall {
    /// Per-instance safety config for path-policy enforcement.
    pub safety_config: Option<SafetyConfig>,
}

impl YarnInstall {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_safety_config(config: SafetyConfig) -> Self {
        Self {
            safety_config: Some(config),
        }
    }
}

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
                },
                "timeout_secs": {
                    "type": "integer",
                    "description": "Timeout in seconds (default: 300)"
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

        let timeout_secs = args
            .get("timeout_secs")
            .and_then(|v| v.as_u64())
            .unwrap_or(300);

        // `yarn add/install` mutates the working directory (node_modules,
        // yarn.lock) — the cwd obeys the workspace path policy like other
        // package-manager write targets (2026-09-21 review sweep).
        let safety = resolve_safety_config(self.safety_config.as_ref());
        validate_tool_path(path, &safety)?;

        let mut cmd = Command::new("yarn");
        crate::safety::process_env::sanitize_command_env(&mut cmd);
        crate::tools::workspace_root::CommandRootExt::in_workspace_root(&mut cmd);

        if packages.is_empty() {
            cmd.arg("install");
        } else {
            cmd.arg("add");
            cmd.args(&packages);
            if dev {
                cmd.arg("--dev");
            }
        }

        cmd.current_dir(crate::tools::workspace_root::anchor(path));

        let output = crate::tools::process_guard::run_command_bounded(
            cmd,
            std::time::Duration::from_secs(timeout_secs),
            MAX_PACKAGE_OUTPUT_SIZE,
        )
        .await
        .map_err(|e| match e {
            crate::tools::process_guard::CommandRunError::Timeout(_) => {
                anyhow::anyhow!("yarn install timed out (the yarn process was killed)")
            }
            other => anyhow::anyhow!("Failed to run yarn: {}", other),
        })?;

        let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();

        Ok(json!({
            "success": output.success(),
            "packages": if packages.is_empty() { "all from package.json".to_string() } else { packages.join(", ") },
            "stdout": truncate_output(&stdout, 2000),
            "stderr": truncate_output(&stderr, 1000),
            "exit_code": output.exit_code()
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
        .sanitized_env()
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
    super::truncate_output(output, max_len)
}

#[cfg(test)]
#[path = "../../tests/unit/tools/package/package_test.rs"]
mod tests;
