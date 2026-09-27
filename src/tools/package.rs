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

use crate::tools::argv_guard::{reject_flag_like_operand, reject_flag_like_operands};

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
                    "description": "Registry package names to install, optionally with @version/range/tag (e.g., ['express', 'lodash@4.17.21', '@types/node@^20']). URLs, git/GitHub specs, file:/link: paths, tarballs and npm: aliases are refused. If empty, installs from package.json"
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

        // Package specs are positional: "--prefix=/outside", "--global" or
        // "--registry=http://evil/" would become npm options (review, 0.9.2).
        reject_flag_like_operands(
            "npm_install",
            "packages",
            packages.iter().map(String::as_str),
        )?;
        // ...and a non-option spec can still name a source (`github:u/r`,
        // `https://….tgz`, `file:../x`): registry names only.
        for spec in &packages {
            check_npm_registry_spec("npm_install", spec)?;
        }

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
        // `script` precedes the `--` terminator, so "--prefix=/x" or
        // "--scripts-prepend-node-path" would be read as npm options.
        reject_flag_like_operand("npm_run", "script", Some(script))?;

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
                    "description": "Registry package names to install, optionally with extras and version constraints (e.g., ['requests', 'flask==2.0.0', 'uvicorn[standard]>=0.20']). Direct references (name @ url), URLs, VCS specs, local paths and archives are refused"
                },
                "requirements": {
                    "type": "string",
                    "description": "Path to requirements.txt file. Refused if it (or a -r/-c include) sets an index, find-links, trusted host, editable install, or a URL/VCS/path requirement"
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
        // Requirement specs are positional: "--target=~/.ssh",
        // "--index-url=http://evil/" or "--prefix=/x" would become pip
        // options (review, 0.9.2).
        reject_flag_like_operands(
            "pip_install",
            "packages",
            packages.iter().map(String::as_str),
        )?;
        reject_flag_like_operand("pip_install", "requirements", requirements)?;
        // A spec can name a source without being an option: `name @ url`,
        // `git+https://…`, URLs, paths, wheels.
        for spec in &packages {
            check_pip_registry_spec("pip_install", spec)?;
        }
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
                // The file (and every `-r`/`-c` include) is scanned for
                // index/find-links/trusted-host/editable/URL lines before
                // pip reads it.
                let anchored = crate::tools::workspace_root::anchor(req_file);
                check_requirements_file("pip_install", Path::new(&anchored), &safety, 0)?;
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
                    "description": "Registry package names to install, optionally with @version/range/tag. URLs, git/GitHub specs, file:/link: paths and tarballs are refused. If empty, installs from package.json"
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

        // "--cwd=/outside", "--global-folder=/x" or "--registry=..." would
        // become yarn options (review, 0.9.2).
        reject_flag_like_operands(
            "yarn_install",
            "packages",
            packages.iter().map(String::as_str),
        )?;
        for spec in &packages {
            check_npm_registry_spec("yarn_install", spec)?;
        }

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
// Package source guards
// ============================================================================
//
// `reject_flag_like_operands` (0.9.2) stops a spec from becoming a pip/npm/
// yarn OPTION, but a spec can itself name where code comes from: PEP 508
// direct references (`pkg @ https://…`), `git+https://…`, bare URLs, local
// paths and wheels/archives for pip; `github:user/repo`, `user/repo`,
// `https://….tgz`, `file:../x`, `git+ssh://…` and `npm:` aliases for npm and
// yarn. A requirements file can switch the index for EVERY package
// (`--index-url`, `--extra-index-url`, `-f/--find-links`, `--trusted-host`,
// `--no-index`) or install from a URL (`-e <url>`). Yolo and Daemon run these
// tools without confirmation, so the tools install only registry names with
// optional extras and version constraints. There is no opt-in on these
// tools: installing from another source is refused with a message naming
// the spec, so the user can run it explicitly.

/// The refusal for a non-registry package spec.
fn source_refusal(tool: &str, spec: &str, why: &str) -> anyhow::Error {
    anyhow::anyhow!(
        "{tool} refuses {spec:?}: {why}. This tool installs only registry package names \
         (with optional extras / version constraints); installing from a URL, VCS \
         repository, local path, archive or custom index must be run by the user explicitly"
    )
}

/// Why `spec` is not a registry spec, in words the model can act on.
fn non_registry_reason(spec: &str) -> &'static str {
    let lower = spec.to_ascii_lowercase();
    if lower.contains("://")
        || lower.starts_with("git+")
        || lower.starts_with("git:")
        || lower.starts_with("github:")
        || lower.starts_with("gitlab:")
        || lower.starts_with("bitbucket:")
        || lower.starts_with("gist:")
    {
        "it names a URL or VCS repository"
    } else if lower.starts_with("file:") || lower.starts_with("link:") {
        "it names a local path"
    } else if lower.starts_with("npm:") || lower.contains("@npm:") {
        "it is a package alias"
    } else if is_archive_name(&lower) {
        "it names a package archive"
    } else if lower.starts_with('.')
        || lower.starts_with('/')
        || lower.starts_with('~')
        || lower.contains('\\')
    {
        "it names a local path"
    } else {
        "it is not a registry package name with an optional version constraint"
    }
}

/// A file name pip/npm install as a local package archive.
fn is_archive_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    [
        ".whl", ".tar.gz", ".tgz", ".tar", ".zip", ".tar.bz2", ".tar.xz", ".egg",
    ]
    .iter()
    .any(|ext| lower.ends_with(ext))
}

fn is_name_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-')
}

/// A PEP 508 name: alphanumerics with `.`, `_`, `-` inside.
fn is_pip_name(name: &str) -> bool {
    !name.is_empty()
        && name.chars().all(is_name_char)
        && name.starts_with(|c: char| c.is_ascii_alphanumeric())
        && name.ends_with(|c: char| c.is_ascii_alphanumeric())
}

/// One PEP 440 version clause (`>=1.2`, `==2.*`, `~=3.1`, `===x`).
fn is_pip_version_clause(clause: &str) -> bool {
    const OPS: [&str; 8] = ["===", "~=", "==", "!=", "<=", ">=", "<", ">"];
    let clause = clause.trim();
    let Some(op) = OPS.iter().find(|op| clause.starts_with(**op)) else {
        return false;
    };
    let version = clause[op.len()..].trim();
    !version.is_empty()
        && version
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '*' | '+' | '!' | '_' | '-'))
}

/// Refuse a pip requirement spec that is not `name[extras] <constraints>
/// [; markers]` — i.e. any direct reference (`name @ url`), URL, VCS spec,
/// local path or archive.
pub(crate) fn check_pip_registry_spec(tool: &str, spec: &str) -> Result<()> {
    // Environment markers select WHEN a requirement applies, never where it
    // comes from; everything that can name a source sits before the `;`.
    let body = spec.split(';').next().unwrap_or_default().trim();
    let refuse = || Err(source_refusal(tool, spec, non_registry_reason(body)));
    if body.contains('@') {
        return Err(source_refusal(
            tool,
            spec,
            "it is a direct reference (`name @ <url or path>`)",
        ));
    }
    let name_end = body.find(|c: char| !is_name_char(c)).unwrap_or(body.len());
    // `evil-1.0-py3-none-any.whl` / `evil.tar.gz` are made of name chars but
    // pip installs them as local archive files.
    let name = &body[..name_end];
    if !is_pip_name(name) || is_archive_name(name) {
        return refuse();
    }
    let mut rest = body[name_end..].trim_start();
    if let Some(extras) = rest.strip_prefix('[') {
        let Some(close) = extras.find(']') else {
            return refuse();
        };
        let names_ok = extras[..close]
            .split(',')
            .map(str::trim)
            .all(|e| e.is_empty() || is_pip_name(e));
        if !names_ok {
            return refuse();
        }
        rest = extras[close + 1..].trim_start();
    }
    // `name (>=1.0)` is legacy but valid.
    let rest = rest
        .strip_prefix('(')
        .and_then(|r| r.strip_suffix(')'))
        .unwrap_or(rest)
        .trim();
    if rest.is_empty() || rest.split(',').all(is_pip_version_clause) {
        Ok(())
    } else {
        refuse()
    }
}

/// An npm package name, optionally scoped (`@scope/name`).
fn is_npm_name(name: &str) -> bool {
    let seg_ok = |s: &str| {
        !s.is_empty()
            && s.starts_with(|c: char| c.is_ascii_alphanumeric())
            && s.chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-' | '~'))
    };
    match name.strip_prefix('@') {
        Some(scoped) => {
            matches!(scoped.split_once('/'), Some((scope, pkg)) if seg_ok(scope) && seg_ok(pkg))
        }
        None => seg_ok(name),
    }
}

/// Refuse an npm/yarn spec that is not `name`, `@scope/name`, or either
/// with `@<version, range or dist-tag>`: no URL, git/GitHub shorthand,
/// `file:`/`link:` path, tarball or `npm:` alias.
pub(crate) fn check_npm_registry_spec(tool: &str, spec: &str) -> Result<()> {
    let s = spec.trim();
    // The version separator is the first `@` after an optional scope `@`.
    let split_at = match s.strip_prefix('@') {
        Some(rest) => rest.find('@').map(|i| i + 1),
        None => s.find('@'),
    };
    let (name, version) = match split_at {
        Some(i) => (&s[..i], Some(&s[i + 1..])),
        None => (s, None),
    };
    if !is_npm_name(name) || is_archive_name(name) {
        return Err(source_refusal(tool, spec, non_registry_reason(s)));
    }
    if let Some(version) = version {
        let ok = !version.trim().is_empty()
            && version.chars().all(|c| {
                c.is_ascii_alphanumeric()
                    || matches!(
                        c,
                        '.' | '-' | '+' | '*' | '^' | '~' | '<' | '>' | '=' | '|' | ' ' | '_'
                    )
            });
        if !ok {
            return Err(source_refusal(tool, spec, non_registry_reason(version)));
        }
    }
    Ok(())
}

/// Requirements-file options that only tune how registry packages are
/// picked, never where they come from. `-r`/`-c` includes are followed and
/// scanned; every other option (`--index-url`, `--extra-index-url`,
/// `-f/--find-links`, `--trusted-host`, `--no-index`, `-e/--editable`,
/// `--config-settings`, `--global-option`, ...) is refused.
const HARMLESS_REQUIREMENT_OPTIONS: [&str; 5] = [
    "--pre",
    "--prefer-binary",
    "--require-hashes",
    "--only-binary",
    "--no-binary",
];

/// Nested `-r`/`-c` include depth before the scan gives up (refuses).
const MAX_REQUIREMENTS_DEPTH: usize = 8;

/// The logical lines of a requirements file: `\`-continuations joined and
/// comments (`#` at line start or after whitespace) removed, as pip reads it.
fn requirement_lines(content: &str) -> Vec<String> {
    let mut lines = Vec::new();
    let mut current = String::new();
    for raw in content.lines() {
        if let Some(head) = raw.strip_suffix('\\') {
            current.push_str(head);
            continue;
        }
        current.push_str(raw);
        let line = std::mem::take(&mut current);
        let without_comment = match line.find('#') {
            Some(0) => "",
            Some(i) if line[..i].ends_with(char::is_whitespace) => &line[..i],
            _ => line.as_str(),
        };
        let trimmed = without_comment.trim();
        if !trimmed.is_empty() {
            lines.push(trimmed.to_string());
        }
    }
    if !current.trim().is_empty() {
        lines.push(current.trim().to_string());
    }
    lines
}

/// Scan a requirements file (and every `-r`/`-c` file it includes) for
/// anything that changes where pip fetches code from, before pip runs.
pub(crate) fn check_requirements_file(
    tool: &str,
    path: &Path,
    safety: &SafetyConfig,
    depth: usize,
) -> Result<()> {
    let shown = path.display().to_string();
    if depth > MAX_REQUIREMENTS_DEPTH {
        anyhow::bail!(
            "{tool} refuses requirements file {shown:?}: `-r`/`-c` includes nest deeper \
             than {MAX_REQUIREMENTS_DEPTH} levels"
        );
    }
    let content = std::fs::read_to_string(path).with_context(|| {
        format!("{tool} could not read requirements file {shown:?} to check its sources")
    })?;
    let refuse = |line: &str, why: &str| -> anyhow::Error {
        anyhow::anyhow!(
            "{tool} refuses requirements file {shown:?}: line {line:?} {why}. Only registry \
             package names (with optional extras / version constraints / hashes) are \
             installed by this tool; a custom index, find-links, trusted host, editable or \
             URL/VCS/path install must be run by the user explicitly"
        )
    };
    for line in requirement_lines(&content) {
        if line.contains("${") {
            return Err(refuse(&line, "uses environment-variable substitution"));
        }
        if line.starts_with('-') {
            let (opt, value) = match line.split_once(|c: char| c == '=' || c.is_whitespace()) {
                Some((o, v)) => (o, v.trim()),
                None => (line.as_str(), ""),
            };
            // `-rfile` / `-cfile` short forms carry the value inline.
            let (opt, value) = match opt {
                o if o.len() > 2
                    && (o.starts_with("-r") || o.starts_with("-c"))
                    && !o.starts_with("--") =>
                {
                    (&o[..2], o[2..].trim())
                }
                o => (o, value),
            };
            match opt {
                "-r" | "--requirement" | "-c" | "--constraint" => {
                    if value.is_empty() || value.contains("://") {
                        return Err(refuse(&line, "includes a file that is not a local path"));
                    }
                    let base = path.parent().unwrap_or_else(|| Path::new("."));
                    let nested = base.join(value);
                    validate_tool_path(&nested.to_string_lossy(), safety)?;
                    check_requirements_file(tool, &nested, safety, depth + 1)?;
                }
                o if HARMLESS_REQUIREMENT_OPTIONS.contains(&o) => {}
                o => {
                    return Err(refuse(
                        &line,
                        &format!("sets `{o}`, which changes where packages come from"),
                    ))
                }
            }
            continue;
        }
        // `spec --hash=sha256:...` : per-requirement hashes only.
        let mut tokens = line.split_whitespace().peekable();
        let mut spec_tokens = Vec::new();
        while let Some(t) = tokens.peek() {
            if t.starts_with('-') {
                break;
            }
            spec_tokens.push(*t);
            tokens.next();
        }
        while let Some(t) = tokens.next() {
            if t == "--hash" {
                tokens.next();
            } else if !t.starts_with("--hash=") {
                return Err(refuse(
                    &line,
                    &format!("sets the per-requirement option `{t}`"),
                ));
            }
        }
        check_pip_registry_spec(tool, &spec_tokens.join(" "))
            .map_err(|e| anyhow::anyhow!("requirements file {shown:?}: {e}"))?;
    }
    Ok(())
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
