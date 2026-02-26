//! Container Tools (Docker & Podman)
//!
//! Tools for managing containers with automatic runtime detection.
//! Supports both Docker and Podman (CLI-compatible).

use anyhow::{Context, Result};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::process::Stdio;
use tokio::process::Command;

use super::Tool;

// ============================================================================
// Runtime Detection
// ============================================================================

/// Detected container runtime
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContainerRuntime {
    Docker,
    Podman,
}

impl ContainerRuntime {
    fn command(&self) -> &'static str {
        match self {
            ContainerRuntime::Docker => "docker",
            ContainerRuntime::Podman => "podman",
        }
    }
}

/// Detect available container runtime (prefers Docker, falls back to Podman)
async fn detect_runtime() -> Result<ContainerRuntime> {
    // Try Docker first
    if Command::new("docker")
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .await
        .map(|s| s.success())
        .unwrap_or(false)
    {
        return Ok(ContainerRuntime::Docker);
    }

    // Try Podman
    if Command::new("podman")
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .await
        .map(|s| s.success())
        .unwrap_or(false)
    {
        return Ok(ContainerRuntime::Podman);
    }

    Err(anyhow::anyhow!(
        "No container runtime found. Please install Docker or Podman."
    ))
}

/// Get container runtime, with optional override
async fn get_runtime(preferred: Option<&str>) -> Result<ContainerRuntime> {
    match preferred {
        Some("docker") => Ok(ContainerRuntime::Docker),
        Some("podman") => Ok(ContainerRuntime::Podman),
        _ => detect_runtime().await,
    }
}

// ============================================================================
// Input Validation Helpers
// ============================================================================

const SHELL_METACHARACTERS: &[char] = &[
    '`', '$', '(', ')', '|', ';', '&', '!', '<', '>', '\n', '\r', '\0',
];

fn validate_port_mapping(mapping: &str) -> bool {
    let (port_part, proto) = if let Some(idx) = mapping.rfind('/') {
        let (p, pr) = mapping.split_at(idx);
        (p, Some(&pr[1..]))
    } else {
        (mapping, None)
    };
    if let Some(proto) = proto {
        if proto != "tcp" && proto != "udp" {
            return false;
        }
    }
    if mapping.contains(SHELL_METACHARACTERS) {
        return false;
    }
    let parts: Vec<&str> = port_part.split(':').collect();
    match parts.len() {
        2 => is_valid_port(parts[0]) && is_valid_port(parts[1]),
        3 => {
            let ip = parts[0];
            !ip.is_empty()
                && ip.chars().all(|c| {
                    c.is_ascii_alphanumeric() || c == '.' || c == ':' || c == '[' || c == ']'
                })
                && is_valid_port(parts[1])
                && is_valid_port(parts[2])
        }
        _ => false,
    }
}

fn is_valid_port(s: &str) -> bool {
    if s.is_empty() {
        return false;
    }
    matches!(s.parse::<u16>(), Ok(p) if p >= 1)
}

fn validate_volume_spec(spec: &str) -> bool {
    if spec.contains(SHELL_METACHARACTERS) {
        return false;
    }
    let parts: Vec<&str> = spec.splitn(3, ':').collect();
    match parts.len() {
        2 => {
            let host = parts[0];
            let container = parts[1];
            !host.is_empty() && !container.is_empty() && container.starts_with('/')
        }
        3 => {
            let host = parts[0];
            let container = parts[1];
            let opts = parts[2];
            !host.is_empty()
                && !container.is_empty()
                && container.starts_with('/')
                && matches!(
                    opts,
                    "ro" | "rw" | "z" | "Z" | "ro,z" | "rw,z" | "ro,Z" | "rw,Z"
                )
        }
        _ => false,
    }
}

// ============================================================================
// Container Run
// ============================================================================

/// Run a container
pub struct ContainerRun;

#[async_trait]
impl Tool for ContainerRun {
    fn name(&self) -> &str {
        "container_run"
    }

    fn description(&self) -> &str {
        "Run a container from an image (docker run / podman run)"
    }

    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "image": {
                    "type": "string",
                    "description": "Container image to run (e.g., 'nginx:latest', 'python:3.11')"
                },
                "name": {
                    "type": "string",
                    "description": "Container name (optional)"
                },
                "command": {
                    "type": "array",
                    "items": {"type": "string"},
                    "description": "Command to run in container"
                },
                "ports": {
                    "type": "array",
                    "items": {"type": "string"},
                    "description": "Port mappings (e.g., ['8080:80', '3000:3000'])"
                },
                "volumes": {
                    "type": "array",
                    "items": {"type": "string"},
                    "description": "Volume mounts (e.g., ['./data:/data', '/host/path:/container/path'])"
                },
                "env": {
                    "type": "object",
                    "description": "Environment variables (e.g., {\"NODE_ENV\": \"production\"})"
                },
                "detach": {
                    "type": "boolean",
                    "description": "Run in background (default: true)"
                },
                "rm": {
                    "type": "boolean",
                    "description": "Remove container when it exits (default: false)"
                },
                "network": {
                    "type": "string",
                    "description": "Network to connect to (e.g., 'host', 'bridge', custom network)"
                },
                "workdir": {
                    "type": "string",
                    "description": "Working directory inside the container"
                },
                "runtime": {
                    "type": "string",
                    "enum": ["docker", "podman", "auto"],
                    "description": "Container runtime to use (default: auto-detect)"
                }
            },
            "required": ["image"]
        })
    }

    async fn execute(&self, args: Value) -> Result<Value> {
        let image = args
            .get("image")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("image is required"))?;

        let runtime = get_runtime(args.get("runtime").and_then(|v| v.as_str())).await?;
        let mut cmd = Command::new(runtime.command());
        cmd.arg("run");

        // Container name
        if let Some(name) = args.get("name").and_then(|v| v.as_str()) {
            cmd.args(["--name", name]);
        }

        // Detach mode (default: true)
        let detach = args.get("detach").and_then(|v| v.as_bool()).unwrap_or(true);
        if detach {
            cmd.arg("-d");
        }

        // Remove on exit
        if args.get("rm").and_then(|v| v.as_bool()).unwrap_or(false) {
            cmd.arg("--rm");
        }

        // Port mappings -- validate to prevent argument injection
        if let Some(ports) = args.get("ports").and_then(|v| v.as_array()) {
            for port in ports {
                if let Some(p) = port.as_str() {
                    if !validate_port_mapping(p) {
                        anyhow::bail!(
                            "Invalid port mapping '{}'. Expected: HOST_PORT:CONTAINER_PORT[/tcp|udp]",
                            p
                        );
                    }
                    cmd.args(["-p", p]);
                }
            }
        }

        // Volume mounts -- validate to prevent argument injection
        if let Some(volumes) = args.get("volumes").and_then(|v| v.as_array()) {
            for vol in volumes {
                if let Some(v) = vol.as_str() {
                    if !validate_volume_spec(v) {
                        anyhow::bail!(
                            "Invalid volume spec '{}'. Expected: HOST_PATH:CONTAINER_PATH[:ro|rw]",
                            v
                        );
                    }
                    cmd.args(["-v", v]);
                }
            }
        }

        // Environment variables -- validate names and values
        if let Some(env) = args.get("env").and_then(|v| v.as_object()) {
            for (key, val) in env {
                if let Some(v) = val.as_str() {
                    if !key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') || key.is_empty()
                    {
                        anyhow::bail!(
                            "Invalid env var name '{}'. Only alphanumeric and underscores allowed.",
                            key
                        );
                    }
                    if v.contains('\0') {
                        anyhow::bail!("Env var value for '{}' must not contain null bytes", key);
                    }
                    cmd.args(["-e", &format!("{}={}", key, v)]);
                }
            }
        }

        // Network
        if let Some(network) = args.get("network").and_then(|v| v.as_str()) {
            cmd.args(["--network", network]);
        }

        // Working directory
        if let Some(workdir) = args.get("workdir").and_then(|v| v.as_str()) {
            cmd.args(["-w", workdir]);
        }

        // Image
        cmd.arg(image);

        // Command
        if let Some(command) = args.get("command").and_then(|v| v.as_array()) {
            for arg in command {
                if let Some(a) = arg.as_str() {
                    cmd.arg(a);
                }
            }
        }

        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        let output = cmd.output().await.context("Failed to run container")?;

        let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();

        // Container ID is in stdout when detached
        let container_id = stdout.trim().to_string();

        Ok(json!({
            "success": output.status.success(),
            "runtime": format!("{:?}", runtime),
            "image": image,
            "container_id": if container_id.len() >= 12 { Some(&container_id[..12]) } else { Some(container_id.as_str()) },
            "detached": detach,
            "stdout": truncate_output(&stdout, 2000),
            "stderr": truncate_output(&stderr, 1000),
            "exit_code": output.status.code()
        }))
    }
}

// ============================================================================
// Container Stop
// ============================================================================

/// Stop a running container
pub struct ContainerStop;

#[async_trait]
impl Tool for ContainerStop {
    fn name(&self) -> &str {
        "container_stop"
    }

    fn description(&self) -> &str {
        "Stop a running container"
    }

    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "container": {
                    "type": "string",
                    "description": "Container ID or name"
                },
                "timeout": {
                    "type": "integer",
                    "description": "Seconds to wait before killing (default: 10)"
                },
                "runtime": {
                    "type": "string",
                    "enum": ["docker", "podman", "auto"],
                    "description": "Container runtime to use"
                }
            },
            "required": ["container"]
        })
    }

    async fn execute(&self, args: Value) -> Result<Value> {
        let container = args
            .get("container")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("container is required"))?;

        let runtime = get_runtime(args.get("runtime").and_then(|v| v.as_str())).await?;
        let timeout = args.get("timeout").and_then(|v| v.as_u64()).unwrap_or(10);

        let mut cmd = Command::new(runtime.command());
        cmd.args(["stop", "-t", &timeout.to_string(), container]);
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        let output = cmd.output().await.context("Failed to stop container")?;

        let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();

        Ok(json!({
            "success": output.status.success(),
            "runtime": format!("{:?}", runtime),
            "container": container,
            "stdout": stdout.trim(),
            "stderr": truncate_output(&stderr, 500),
            "exit_code": output.status.code()
        }))
    }
}

// ============================================================================
// Container List
// ============================================================================

/// List containers
pub struct ContainerList;

#[derive(Debug, Serialize, Deserialize)]
struct ContainerInfo {
    id: String,
    image: String,
    command: String,
    created: String,
    status: String,
    ports: String,
    names: String,
}

#[async_trait]
impl Tool for ContainerList {
    fn name(&self) -> &str {
        "container_list"
    }

    fn description(&self) -> &str {
        "List containers (running by default, or all with 'all: true')"
    }

    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "all": {
                    "type": "boolean",
                    "description": "Show all containers (default: only running)"
                },
                "filter": {
                    "type": "string",
                    "description": "Filter by name, image, or status (e.g., 'name=myapp', 'status=running')"
                },
                "runtime": {
                    "type": "string",
                    "enum": ["docker", "podman", "auto"],
                    "description": "Container runtime to use"
                }
            }
        })
    }

    async fn execute(&self, args: Value) -> Result<Value> {
        let runtime = get_runtime(args.get("runtime").and_then(|v| v.as_str())).await?;
        let all = args.get("all").and_then(|v| v.as_bool()).unwrap_or(false);

        let mut cmd = Command::new(runtime.command());
        cmd.args(["ps", "--format", "{{.ID}}\t{{.Image}}\t{{.Command}}\t{{.CreatedAt}}\t{{.Status}}\t{{.Ports}}\t{{.Names}}"]);

        if all {
            cmd.arg("-a");
        }

        if let Some(filter) = args.get("filter").and_then(|v| v.as_str()) {
            cmd.args(["--filter", filter]);
        }

        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        let output = cmd.output().await.context("Failed to list containers")?;

        let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();

        let containers: Vec<ContainerInfo> = stdout
            .lines()
            .filter(|line| !line.is_empty())
            .filter_map(|line| {
                let parts: Vec<&str> = line.split('\t').collect();
                if parts.len() >= 7 {
                    Some(ContainerInfo {
                        id: parts[0].to_string(),
                        image: parts[1].to_string(),
                        command: parts[2].to_string(),
                        created: parts[3].to_string(),
                        status: parts[4].to_string(),
                        ports: parts[5].to_string(),
                        names: parts[6].to_string(),
                    })
                } else {
                    None
                }
            })
            .collect();

        Ok(json!({
            "success": output.status.success(),
            "runtime": format!("{:?}", runtime),
            "containers": containers,
            "count": containers.len(),
            "show_all": all,
            "stderr": if stderr.is_empty() { None } else { Some(truncate_output(&stderr, 500)) }
        }))
    }
}

// ============================================================================
// Container Logs
// ============================================================================

/// Get container logs
pub struct ContainerLogs;

#[async_trait]
impl Tool for ContainerLogs {
    fn name(&self) -> &str {
        "container_logs"
    }

    fn description(&self) -> &str {
        "Get logs from a container"
    }

    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "container": {
                    "type": "string",
                    "description": "Container ID or name"
                },
                "tail": {
                    "type": "integer",
                    "description": "Number of lines to show from end (default: 100)"
                },
                "since": {
                    "type": "string",
                    "description": "Show logs since timestamp (e.g., '2023-01-01', '10m', '1h')"
                },
                "timestamps": {
                    "type": "boolean",
                    "description": "Show timestamps (default: false)"
                },
                "runtime": {
                    "type": "string",
                    "enum": ["docker", "podman", "auto"],
                    "description": "Container runtime to use"
                }
            },
            "required": ["container"]
        })
    }

    async fn execute(&self, args: Value) -> Result<Value> {
        let container = args
            .get("container")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("container is required"))?;

        let runtime = get_runtime(args.get("runtime").and_then(|v| v.as_str())).await?;
        let tail = args.get("tail").and_then(|v| v.as_u64()).unwrap_or(100);

        let mut cmd = Command::new(runtime.command());
        cmd.args(["logs", "--tail", &tail.to_string()]);

        if let Some(since) = args.get("since").and_then(|v| v.as_str()) {
            cmd.args(["--since", since]);
        }

        if args
            .get("timestamps")
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
        {
            cmd.arg("-t");
        }

        cmd.arg(container);
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        let output = cmd.output().await.context("Failed to get container logs")?;

        let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();

        // Container logs often go to stderr
        let logs = if stdout.is_empty() && !stderr.is_empty() {
            stderr.clone()
        } else {
            format!("{}\n{}", stdout, stderr)
        };

        Ok(json!({
            "success": output.status.success(),
            "runtime": format!("{:?}", runtime),
            "container": container,
            "logs": truncate_output(&logs, 5000),
            "lines": logs.lines().count(),
            "exit_code": output.status.code()
        }))
    }
}

// ============================================================================
// Container Exec
// ============================================================================

/// Execute a command in a running container
pub struct ContainerExec;

#[async_trait]
impl Tool for ContainerExec {
    fn name(&self) -> &str {
        "container_exec"
    }

    fn description(&self) -> &str {
        "Execute a command inside a running container"
    }

    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "container": {
                    "type": "string",
                    "description": "Container ID or name"
                },
                "command": {
                    "type": "array",
                    "items": {"type": "string"},
                    "description": "Command and arguments to execute (e.g., ['ls', '-la'])"
                },
                "workdir": {
                    "type": "string",
                    "description": "Working directory inside container"
                },
                "env": {
                    "type": "object",
                    "description": "Environment variables for the command"
                },
                "user": {
                    "type": "string",
                    "description": "User to run command as (e.g., 'root', '1000:1000')"
                },
                "runtime": {
                    "type": "string",
                    "enum": ["docker", "podman", "auto"],
                    "description": "Container runtime to use"
                }
            },
            "required": ["container", "command"]
        })
    }

    async fn execute(&self, args: Value) -> Result<Value> {
        let container = args
            .get("container")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("container is required"))?;

        let command: Vec<String> = args
            .get("command")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .ok_or_else(|| anyhow::anyhow!("command is required"))?;

        if command.is_empty() {
            return Err(anyhow::anyhow!("command cannot be empty"));
        }

        let runtime = get_runtime(args.get("runtime").and_then(|v| v.as_str())).await?;

        let mut cmd = Command::new(runtime.command());
        cmd.arg("exec");

        // Working directory
        if let Some(workdir) = args.get("workdir").and_then(|v| v.as_str()) {
            cmd.args(["-w", workdir]);
        }

        // User
        if let Some(user) = args.get("user").and_then(|v| v.as_str()) {
            cmd.args(["-u", user]);
        }

        // Environment variables
        if let Some(env) = args.get("env").and_then(|v| v.as_object()) {
            for (key, val) in env {
                if let Some(v) = val.as_str() {
                    cmd.args(["-e", &format!("{}={}", key, v)]);
                }
            }
        }

        cmd.arg(container);
        cmd.args(&command);
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        let output = cmd.output().await.context("Failed to exec in container")?;

        let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();

        Ok(json!({
            "success": output.status.success(),
            "runtime": format!("{:?}", runtime),
            "container": container,
            "command": command.join(" "),
            "stdout": truncate_output(&stdout, 3000),
            "stderr": truncate_output(&stderr, 1000),
            "exit_code": output.status.code()
        }))
    }
}

// ============================================================================
// Container Build
// ============================================================================

/// Build a container image from Dockerfile
pub struct ContainerBuild;

#[async_trait]
impl Tool for ContainerBuild {
    fn name(&self) -> &str {
        "container_build"
    }

    fn description(&self) -> &str {
        "Build a container image from a Dockerfile"
    }

    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "tag": {
                    "type": "string",
                    "description": "Image tag (e.g., 'myapp:latest', 'myregistry/myapp:v1.0')"
                },
                "path": {
                    "type": "string",
                    "description": "Build context path (default: current directory)"
                },
                "dockerfile": {
                    "type": "string",
                    "description": "Path to Dockerfile (default: Dockerfile in context)"
                },
                "build_args": {
                    "type": "object",
                    "description": "Build arguments (e.g., {\"NODE_VERSION\": \"18\"})"
                },
                "no_cache": {
                    "type": "boolean",
                    "description": "Do not use cache (default: false)"
                },
                "target": {
                    "type": "string",
                    "description": "Build target stage (for multi-stage builds)"
                },
                "runtime": {
                    "type": "string",
                    "enum": ["docker", "podman", "auto"],
                    "description": "Container runtime to use"
                }
            },
            "required": ["tag"]
        })
    }

    async fn execute(&self, args: Value) -> Result<Value> {
        let tag = args
            .get("tag")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("tag is required"))?;

        let path = args.get("path").and_then(|v| v.as_str()).unwrap_or(".");

        let runtime = get_runtime(args.get("runtime").and_then(|v| v.as_str())).await?;

        let mut cmd = Command::new(runtime.command());
        cmd.args(["build", "-t", tag]);

        // Dockerfile path
        if let Some(dockerfile) = args.get("dockerfile").and_then(|v| v.as_str()) {
            cmd.args(["-f", dockerfile]);
        }

        // Build args
        if let Some(build_args) = args.get("build_args").and_then(|v| v.as_object()) {
            for (key, val) in build_args {
                if let Some(v) = val.as_str() {
                    cmd.args(["--build-arg", &format!("{}={}", key, v)]);
                }
            }
        }

        // No cache
        if args
            .get("no_cache")
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
        {
            cmd.arg("--no-cache");
        }

        // Target stage
        if let Some(target) = args.get("target").and_then(|v| v.as_str()) {
            cmd.args(["--target", target]);
        }

        cmd.arg(path);
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        let output = tokio::time::timeout(
            std::time::Duration::from_secs(600), // 10 minute timeout for builds
            cmd.output(),
        )
        .await
        .context("Build timed out")?
        .context("Failed to build image")?;

        let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();

        // Parse build output for image ID
        let image_id = parse_build_output(&stdout, &stderr);

        Ok(json!({
            "success": output.status.success(),
            "runtime": format!("{:?}", runtime),
            "tag": tag,
            "image_id": image_id,
            "stdout": truncate_output(&stdout, 3000),
            "stderr": truncate_output(&stderr, 2000),
            "exit_code": output.status.code()
        }))
    }
}

// ============================================================================
// Container Images
// ============================================================================

/// List container images
pub struct ContainerImages;

#[derive(Debug, Serialize, Deserialize)]
struct ImageInfo {
    id: String,
    repository: String,
    tag: String,
    created: String,
    size: String,
}

#[async_trait]
impl Tool for ContainerImages {
    fn name(&self) -> &str {
        "container_images"
    }

    fn description(&self) -> &str {
        "List container images"
    }

    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "filter": {
                    "type": "string",
                    "description": "Filter images (e.g., 'reference=nginx*')"
                },
                "all": {
                    "type": "boolean",
                    "description": "Show all images including intermediate (default: false)"
                },
                "runtime": {
                    "type": "string",
                    "enum": ["docker", "podman", "auto"],
                    "description": "Container runtime to use"
                }
            }
        })
    }

    async fn execute(&self, args: Value) -> Result<Value> {
        let runtime = get_runtime(args.get("runtime").and_then(|v| v.as_str())).await?;
        let all = args.get("all").and_then(|v| v.as_bool()).unwrap_or(false);

        let mut cmd = Command::new(runtime.command());
        cmd.args([
            "images",
            "--format",
            "{{.ID}}\t{{.Repository}}\t{{.Tag}}\t{{.CreatedAt}}\t{{.Size}}",
        ]);

        if all {
            cmd.arg("-a");
        }

        if let Some(filter) = args.get("filter").and_then(|v| v.as_str()) {
            cmd.args(["--filter", filter]);
        }

        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        let output = cmd.output().await.context("Failed to list images")?;

        let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();

        let images: Vec<ImageInfo> = stdout
            .lines()
            .filter(|line| !line.is_empty())
            .filter_map(|line| {
                let parts: Vec<&str> = line.split('\t').collect();
                if parts.len() >= 5 {
                    Some(ImageInfo {
                        id: parts[0].to_string(),
                        repository: parts[1].to_string(),
                        tag: parts[2].to_string(),
                        created: parts[3].to_string(),
                        size: parts[4].to_string(),
                    })
                } else {
                    None
                }
            })
            .collect();

        Ok(json!({
            "success": output.status.success(),
            "runtime": format!("{:?}", runtime),
            "images": images,
            "count": images.len(),
            "stderr": if stderr.is_empty() { None } else { Some(truncate_output(&stderr, 500)) }
        }))
    }
}

// ============================================================================
// Container Pull
// ============================================================================

/// Pull a container image
pub struct ContainerPull;

#[async_trait]
impl Tool for ContainerPull {
    fn name(&self) -> &str {
        "container_pull"
    }

    fn description(&self) -> &str {
        "Pull a container image from a registry"
    }

    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "image": {
                    "type": "string",
                    "description": "Image to pull (e.g., 'nginx:latest', 'python:3.11-slim')"
                },
                "runtime": {
                    "type": "string",
                    "enum": ["docker", "podman", "auto"],
                    "description": "Container runtime to use"
                }
            },
            "required": ["image"]
        })
    }

    async fn execute(&self, args: Value) -> Result<Value> {
        let image = args
            .get("image")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("image is required"))?;

        let runtime = get_runtime(args.get("runtime").and_then(|v| v.as_str())).await?;

        let mut cmd = Command::new(runtime.command());
        cmd.args(["pull", image]);
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        let output = tokio::time::timeout(
            std::time::Duration::from_secs(300), // 5 minute timeout for pulls
            cmd.output(),
        )
        .await
        .context("Pull timed out")?
        .context("Failed to pull image")?;

        let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();

        Ok(json!({
            "success": output.status.success(),
            "runtime": format!("{:?}", runtime),
            "image": image,
            "stdout": truncate_output(&stdout, 2000),
            "stderr": truncate_output(&stderr, 1000),
            "exit_code": output.status.code()
        }))
    }
}

// ============================================================================
// Container Remove
// ============================================================================

/// Remove a container
pub struct ContainerRemove;

#[async_trait]
impl Tool for ContainerRemove {
    fn name(&self) -> &str {
        "container_remove"
    }

    fn description(&self) -> &str {
        "Remove a stopped container (use force to remove running containers)"
    }

    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "container": {
                    "type": "string",
                    "description": "Container ID or name"
                },
                "force": {
                    "type": "boolean",
                    "description": "Force remove even if running (default: false)"
                },
                "volumes": {
                    "type": "boolean",
                    "description": "Remove associated volumes (default: false)"
                },
                "runtime": {
                    "type": "string",
                    "enum": ["docker", "podman", "auto"],
                    "description": "Container runtime to use"
                }
            },
            "required": ["container"]
        })
    }

    async fn execute(&self, args: Value) -> Result<Value> {
        let container = args
            .get("container")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("container is required"))?;

        let runtime = get_runtime(args.get("runtime").and_then(|v| v.as_str())).await?;

        let mut cmd = Command::new(runtime.command());
        cmd.args(["rm"]);

        if args.get("force").and_then(|v| v.as_bool()).unwrap_or(false) {
            cmd.arg("-f");
        }

        if args
            .get("volumes")
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
        {
            cmd.arg("-v");
        }

        cmd.arg(container);
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        let output = cmd.output().await.context("Failed to remove container")?;

        let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();

        Ok(json!({
            "success": output.status.success(),
            "runtime": format!("{:?}", runtime),
            "container": container,
            "removed": output.status.success(),
            "stdout": stdout.trim(),
            "stderr": truncate_output(&stderr, 500),
            "exit_code": output.status.code()
        }))
    }
}

// ============================================================================
// Docker Compose / Podman Compose
// ============================================================================

/// Run docker-compose or podman-compose commands
pub struct ComposeUp;

#[async_trait]
impl Tool for ComposeUp {
    fn name(&self) -> &str {
        "compose_up"
    }

    fn description(&self) -> &str {
        "Start services defined in docker-compose.yml (docker compose up / podman-compose up)"
    }

    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Path to directory containing docker-compose.yml (default: current directory)"
                },
                "file": {
                    "type": "string",
                    "description": "Compose file name (default: docker-compose.yml)"
                },
                "services": {
                    "type": "array",
                    "items": {"type": "string"},
                    "description": "Specific services to start (default: all)"
                },
                "detach": {
                    "type": "boolean",
                    "description": "Run in background (default: true)"
                },
                "build": {
                    "type": "boolean",
                    "description": "Build images before starting (default: false)"
                },
                "runtime": {
                    "type": "string",
                    "enum": ["docker", "podman", "auto"],
                    "description": "Container runtime to use"
                }
            }
        })
    }

    async fn execute(&self, args: Value) -> Result<Value> {
        let path = args.get("path").and_then(|v| v.as_str()).unwrap_or(".");

        let runtime = get_runtime(args.get("runtime").and_then(|v| v.as_str())).await?;

        // Use 'docker compose' (v2) for Docker, 'podman-compose' for Podman
        let (cmd_name, compose_args) = match runtime {
            ContainerRuntime::Docker => ("docker", vec!["compose"]),
            ContainerRuntime::Podman => ("podman-compose", vec![]),
        };

        let mut cmd = Command::new(cmd_name);
        cmd.args(&compose_args);

        // Compose file
        if let Some(file) = args.get("file").and_then(|v| v.as_str()) {
            cmd.args(["-f", file]);
        }

        cmd.arg("up");

        // Detach
        if args.get("detach").and_then(|v| v.as_bool()).unwrap_or(true) {
            cmd.arg("-d");
        }

        // Build
        if args.get("build").and_then(|v| v.as_bool()).unwrap_or(false) {
            cmd.arg("--build");
        }

        // Specific services
        if let Some(services) = args.get("services").and_then(|v| v.as_array()) {
            for service in services {
                if let Some(s) = service.as_str() {
                    cmd.arg(s);
                }
            }
        }

        cmd.current_dir(path);
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        let output = tokio::time::timeout(std::time::Duration::from_secs(300), cmd.output())
            .await
            .context("Compose up timed out")?
            .context("Failed to run compose up")?;

        let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();

        Ok(json!({
            "success": output.status.success(),
            "runtime": format!("{:?}", runtime),
            "path": path,
            "stdout": truncate_output(&stdout, 2000),
            "stderr": truncate_output(&stderr, 1000),
            "exit_code": output.status.code()
        }))
    }
}

/// Stop compose services
pub struct ComposeDown;

#[async_trait]
impl Tool for ComposeDown {
    fn name(&self) -> &str {
        "compose_down"
    }

    fn description(&self) -> &str {
        "Stop and remove containers defined in docker-compose.yml"
    }

    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Path to directory containing docker-compose.yml"
                },
                "file": {
                    "type": "string",
                    "description": "Compose file name (default: docker-compose.yml)"
                },
                "volumes": {
                    "type": "boolean",
                    "description": "Remove named volumes (default: false)"
                },
                "rmi": {
                    "type": "string",
                    "enum": ["all", "local"],
                    "description": "Remove images ('all' or 'local')"
                },
                "runtime": {
                    "type": "string",
                    "enum": ["docker", "podman", "auto"],
                    "description": "Container runtime to use"
                }
            }
        })
    }

    async fn execute(&self, args: Value) -> Result<Value> {
        let path = args.get("path").and_then(|v| v.as_str()).unwrap_or(".");

        let runtime = get_runtime(args.get("runtime").and_then(|v| v.as_str())).await?;

        let (cmd_name, compose_args) = match runtime {
            ContainerRuntime::Docker => ("docker", vec!["compose"]),
            ContainerRuntime::Podman => ("podman-compose", vec![]),
        };

        let mut cmd = Command::new(cmd_name);
        cmd.args(&compose_args);

        if let Some(file) = args.get("file").and_then(|v| v.as_str()) {
            cmd.args(["-f", file]);
        }

        cmd.arg("down");

        if args
            .get("volumes")
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
        {
            cmd.arg("-v");
        }

        if let Some(rmi) = args.get("rmi").and_then(|v| v.as_str()) {
            cmd.args(["--rmi", rmi]);
        }

        cmd.current_dir(path);
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        let output = cmd.output().await.context("Failed to run compose down")?;

        let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();

        Ok(json!({
            "success": output.status.success(),
            "runtime": format!("{:?}", runtime),
            "path": path,
            "stdout": truncate_output(&stdout, 2000),
            "stderr": truncate_output(&stderr, 1000),
            "exit_code": output.status.code()
        }))
    }
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Truncate output to max length
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

/// Parse build output for image ID
fn parse_build_output(stdout: &str, stderr: &str) -> Option<String> {
    let combined = format!("{}\n{}", stdout, stderr);

    // Look for "Successfully built <id>" or "writing image sha256:<id>"
    for line in combined.lines() {
        if line.contains("Successfully built") {
            return line.split_whitespace().last().map(String::from);
        }
        if line.contains("writing image sha256:") {
            if let Some(sha) = line.split("sha256:").nth(1) {
                return Some(sha.split_whitespace().next().unwrap_or(sha).to_string());
            }
        }
    }
    None
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_container_run_schema() {
        let tool = ContainerRun;
        let schema = tool.schema();
        assert!(schema["properties"].get("image").is_some());
        assert!(schema["required"]
            .as_array()
            .unwrap()
            .contains(&json!("image")));
    }

    #[test]
    fn test_container_stop_schema() {
        let tool = ContainerStop;
        let schema = tool.schema();
        assert!(schema["properties"].get("container").is_some());
        assert!(schema["properties"].get("timeout").is_some());
    }

    #[test]
    fn test_container_build_schema() {
        let tool = ContainerBuild;
        let schema = tool.schema();
        assert!(schema["properties"].get("tag").is_some());
        assert!(schema["properties"].get("dockerfile").is_some());
        assert!(schema["properties"].get("build_args").is_some());
    }

    #[test]
    fn test_container_exec_schema() {
        let tool = ContainerExec;
        let schema = tool.schema();
        assert!(schema["properties"].get("container").is_some());
        assert!(schema["properties"].get("command").is_some());
    }

    #[test]
    fn test_tool_names() {
        assert_eq!(ContainerRun.name(), "container_run");
        assert_eq!(ContainerStop.name(), "container_stop");
        assert_eq!(ContainerList.name(), "container_list");
        assert_eq!(ContainerLogs.name(), "container_logs");
        assert_eq!(ContainerExec.name(), "container_exec");
        assert_eq!(ContainerBuild.name(), "container_build");
        assert_eq!(ContainerImages.name(), "container_images");
        assert_eq!(ContainerPull.name(), "container_pull");
        assert_eq!(ContainerRemove.name(), "container_remove");
        assert_eq!(ComposeUp.name(), "compose_up");
        assert_eq!(ComposeDown.name(), "compose_down");
    }

    #[test]
    fn test_tool_descriptions() {
        assert!(!ContainerRun.description().is_empty());
        assert!(ContainerRun.description().contains("container"));
        assert!(!ContainerBuild.description().is_empty());
        assert!(ContainerBuild.description().contains("Dockerfile"));
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
    fn test_parse_build_output_success() {
        let stdout = "Step 1/5 : FROM node:18\nSuccessfully built abc123def456";
        let result = parse_build_output(stdout, "");
        assert_eq!(result, Some("abc123def456".to_string()));
    }

    #[test]
    fn test_parse_build_output_sha256() {
        let stderr = "writing image sha256:abc123def456789 done";
        let result = parse_build_output("", stderr);
        assert_eq!(result, Some("abc123def456789".to_string()));
    }

    #[test]
    fn test_parse_build_output_none() {
        let result = parse_build_output("random output", "more random");
        assert_eq!(result, None);
    }

    #[test]
    fn test_runtime_command() {
        assert_eq!(ContainerRuntime::Docker.command(), "docker");
        assert_eq!(ContainerRuntime::Podman.command(), "podman");
    }

    #[tokio::test]
    async fn test_container_exec_no_command() {
        let tool = ContainerExec;
        let result = tool.execute(json!({"container": "test"})).await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("command is required"));
    }

    #[tokio::test]
    async fn test_container_exec_empty_command() {
        let tool = ContainerExec;
        let result = tool
            .execute(json!({"container": "test", "command": []}))
            .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("cannot be empty"));
    }

    #[tokio::test]
    async fn test_container_run_no_image() {
        let tool = ContainerRun;
        let result = tool.execute(json!({})).await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("image is required"));
    }

    #[test]
    fn test_container_list_schema() {
        let tool = ContainerList;
        let schema = tool.schema();
        assert!(schema["properties"].get("all").is_some());
        assert!(schema["properties"].get("filter").is_some());
        assert!(schema["properties"].get("runtime").is_some());
    }

    #[test]
    fn test_container_logs_schema() {
        let tool = ContainerLogs;
        let schema = tool.schema();
        assert!(schema["properties"].get("container").is_some());
        assert!(schema["properties"].get("tail").is_some());
        assert!(schema["properties"].get("since").is_some());
        assert!(schema["properties"].get("timestamps").is_some());
    }

    #[test]
    fn test_container_images_schema() {
        let tool = ContainerImages;
        let schema = tool.schema();
        assert!(schema["properties"].get("filter").is_some());
        assert!(schema["properties"].get("all").is_some());
    }

    #[test]
    fn test_container_pull_schema() {
        let tool = ContainerPull;
        let schema = tool.schema();
        assert!(schema["properties"].get("image").is_some());
        assert!(schema["required"]
            .as_array()
            .unwrap()
            .contains(&json!("image")));
    }

    #[test]
    fn test_container_remove_schema() {
        let tool = ContainerRemove;
        let schema = tool.schema();
        assert!(schema["properties"].get("container").is_some());
        assert!(schema["properties"].get("force").is_some());
        assert!(schema["properties"].get("volumes").is_some());
    }

    #[test]
    fn test_compose_up_schema() {
        let tool = ComposeUp;
        let schema = tool.schema();
        assert!(schema["properties"].get("path").is_some());
        assert!(schema["properties"].get("file").is_some());
        assert!(schema["properties"].get("services").is_some());
        assert!(schema["properties"].get("detach").is_some());
        assert!(schema["properties"].get("build").is_some());
    }

    #[test]
    fn test_compose_down_schema() {
        let tool = ComposeDown;
        let schema = tool.schema();
        assert!(schema["properties"].get("path").is_some());
        assert!(schema["properties"].get("file").is_some());
        assert!(schema["properties"].get("volumes").is_some());
        assert!(schema["properties"].get("rmi").is_some());
    }

    #[test]
    fn test_container_runtime_debug() {
        let docker = ContainerRuntime::Docker;
        let podman = ContainerRuntime::Podman;
        assert_eq!(format!("{:?}", docker), "Docker");
        assert_eq!(format!("{:?}", podman), "Podman");
    }

    #[test]
    fn test_container_runtime_clone() {
        let docker = ContainerRuntime::Docker;
        let cloned = docker;
        assert_eq!(cloned, ContainerRuntime::Docker);
    }

    #[test]
    fn test_container_runtime_equality() {
        assert_eq!(ContainerRuntime::Docker, ContainerRuntime::Docker);
        assert_eq!(ContainerRuntime::Podman, ContainerRuntime::Podman);
        assert_ne!(ContainerRuntime::Docker, ContainerRuntime::Podman);
    }

    #[tokio::test]
    async fn test_container_stop_no_container() {
        let tool = ContainerStop;
        let result = tool.execute(json!({})).await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("container is required"));
    }

    #[tokio::test]
    async fn test_container_logs_no_container() {
        let tool = ContainerLogs;
        let result = tool.execute(json!({})).await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("container is required"));
    }

    #[tokio::test]
    async fn test_container_exec_no_container() {
        let tool = ContainerExec;
        let result = tool.execute(json!({"command": ["ls"]})).await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("container is required"));
    }

    #[tokio::test]
    async fn test_container_build_no_tag() {
        let tool = ContainerBuild;
        let result = tool.execute(json!({})).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("tag is required"));
    }

    #[tokio::test]
    async fn test_container_pull_no_image() {
        let tool = ContainerPull;
        let result = tool.execute(json!({})).await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("image is required"));
    }

    #[tokio::test]
    async fn test_container_remove_no_container() {
        let tool = ContainerRemove;
        let result = tool.execute(json!({})).await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("container is required"));
    }

    #[test]
    fn test_truncate_output_empty() {
        assert_eq!(truncate_output("", 100), "");
    }

    #[test]
    fn test_truncate_output_exact_length() {
        let s = "12345";
        assert_eq!(truncate_output(s, 5), "12345");
    }

    #[test]
    fn test_parse_build_output_empty() {
        assert_eq!(parse_build_output("", ""), None);
    }

    #[test]
    fn test_all_tool_descriptions_non_empty() {
        assert!(!ContainerRun.description().is_empty());
        assert!(!ContainerStop.description().is_empty());
        assert!(!ContainerList.description().is_empty());
        assert!(!ContainerLogs.description().is_empty());
        assert!(!ContainerExec.description().is_empty());
        assert!(!ContainerBuild.description().is_empty());
        assert!(!ContainerImages.description().is_empty());
        assert!(!ContainerPull.description().is_empty());
        assert!(!ContainerRemove.description().is_empty());
        assert!(!ComposeUp.description().is_empty());
        assert!(!ComposeDown.description().is_empty());
    }

    #[test]
    fn test_container_info_serialization() {
        let info = ContainerInfo {
            id: "abc123".to_string(),
            image: "nginx:latest".to_string(),
            command: "nginx".to_string(),
            created: "2024-01-01".to_string(),
            status: "Up 5 hours".to_string(),
            ports: "80/tcp".to_string(),
            names: "my-nginx".to_string(),
        };
        let json = serde_json::to_string(&info).unwrap();
        assert!(json.contains("abc123"));
        assert!(json.contains("nginx:latest"));
    }

    #[test]
    fn test_image_info_serialization() {
        let info = ImageInfo {
            id: "sha256:abc".to_string(),
            repository: "nginx".to_string(),
            tag: "latest".to_string(),
            created: "2024-01-01".to_string(),
            size: "100MB".to_string(),
        };
        let json = serde_json::to_string(&info).unwrap();
        assert!(json.contains("nginx"));
        assert!(json.contains("latest"));
    }

    #[test]
    fn test_container_run_schema_complete() {
        let tool = ContainerRun;
        let schema = tool.schema();
        assert!(schema["properties"].get("name").is_some());
        assert!(schema["properties"].get("command").is_some());
        assert!(schema["properties"].get("ports").is_some());
        assert!(schema["properties"].get("volumes").is_some());
        assert!(schema["properties"].get("env").is_some());
        assert!(schema["properties"].get("detach").is_some());
        assert!(schema["properties"].get("rm").is_some());
        assert!(schema["properties"].get("network").is_some());
        assert!(schema["properties"].get("workdir").is_some());
        assert!(schema["properties"].get("runtime").is_some());
    }

    #[test]
    fn test_container_build_schema_complete() {
        let tool = ContainerBuild;
        let schema = tool.schema();
        assert!(schema["properties"].get("path").is_some());
        assert!(schema["properties"].get("no_cache").is_some());
        assert!(schema["properties"].get("target").is_some());
    }

    #[test]
    fn test_container_exec_schema_complete() {
        let tool = ContainerExec;
        let schema = tool.schema();
        assert!(schema["properties"].get("workdir").is_some());
        assert!(schema["properties"].get("env").is_some());
        assert!(schema["properties"].get("user").is_some());
    }

    #[test]
    fn test_container_info_debug() {
        let info = ContainerInfo {
            id: "abc123".to_string(),
            image: "nginx:latest".to_string(),
            command: "nginx".to_string(),
            created: "2024-01-01".to_string(),
            status: "Up 5 hours".to_string(),
            ports: "80/tcp".to_string(),
            names: "my-nginx".to_string(),
        };
        let debug = format!("{:?}", info);
        assert!(debug.contains("ContainerInfo"));
        assert!(debug.contains("abc123"));
    }

    #[test]
    fn test_image_info_debug() {
        let info = ImageInfo {
            id: "sha256:abc".to_string(),
            repository: "nginx".to_string(),
            tag: "latest".to_string(),
            created: "2024-01-01".to_string(),
            size: "100MB".to_string(),
        };
        let debug = format!("{:?}", info);
        assert!(debug.contains("ImageInfo"));
        assert!(debug.contains("nginx"));
    }

    #[test]
    fn test_container_info_deserialization() {
        let json = r#"{"id":"abc","image":"nginx","command":"sh","created":"now","status":"running","ports":"80","names":"test"}"#;
        let info: ContainerInfo = serde_json::from_str(json).unwrap();
        assert_eq!(info.id, "abc");
        assert_eq!(info.image, "nginx");
    }

    #[test]
    fn test_image_info_deserialization() {
        let json = r#"{"id":"sha256:abc","repository":"nginx","tag":"latest","created":"now","size":"10MB"}"#;
        let info: ImageInfo = serde_json::from_str(json).unwrap();
        assert_eq!(info.id, "sha256:abc");
        assert_eq!(info.repository, "nginx");
    }

    #[test]
    fn test_truncate_output_unicode() {
        let output = "Hello 世界! 🎉";
        assert_eq!(truncate_output(output, 100), output);
    }

    #[test]
    fn test_parse_build_output_multiple_steps() {
        let stdout = "Step 1/5 : FROM node:18\nStep 2/5 : COPY . .\nStep 3/5 : RUN npm install\nStep 4/5 : EXPOSE 3000\nStep 5/5 : CMD npm start\nSuccessfully built xyz789";
        let result = parse_build_output(stdout, "");
        assert_eq!(result, Some("xyz789".to_string()));
    }

    #[test]
    fn test_parse_build_output_sha256_with_extra_text() {
        let stderr = "some prefix writing image sha256:abc123def456 and some suffix";
        let result = parse_build_output("", stderr);
        assert_eq!(result, Some("abc123def456".to_string()));
    }

    #[test]
    fn test_runtime_copy_trait() {
        let docker = ContainerRuntime::Docker;
        let copy = docker;
        assert_eq!(copy.command(), "docker");
        assert_eq!(docker.command(), "docker"); // original still valid
    }

    #[tokio::test]
    async fn test_get_runtime_docker_override() {
        let result = get_runtime(Some("docker")).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), ContainerRuntime::Docker);
    }

    #[tokio::test]
    async fn test_get_runtime_podman_override() {
        let result = get_runtime(Some("podman")).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), ContainerRuntime::Podman);
    }

    #[test]
    fn test_container_stop_schema_timeout() {
        let tool = ContainerStop;
        let schema = tool.schema();
        let timeout = &schema["properties"]["timeout"];
        assert_eq!(timeout["type"], "integer");
    }

    #[test]
    fn test_container_logs_schema_timestamps() {
        let tool = ContainerLogs;
        let schema = tool.schema();
        let timestamps = &schema["properties"]["timestamps"];
        assert_eq!(timestamps["type"], "boolean");
    }

    #[test]
    fn test_compose_up_schema_services() {
        let tool = ComposeUp;
        let schema = tool.schema();
        let services = &schema["properties"]["services"];
        assert_eq!(services["type"], "array");
    }

    #[test]
    fn test_compose_down_schema_rmi() {
        let tool = ComposeDown;
        let schema = tool.schema();
        let rmi = &schema["properties"]["rmi"];
        assert_eq!(rmi["type"], "string");
        assert!(rmi["enum"].as_array().unwrap().contains(&json!("all")));
        assert!(rmi["enum"].as_array().unwrap().contains(&json!("local")));
    }

    #[test]
    fn test_container_runtime_both_variants() {
        let docker = ContainerRuntime::Docker;
        let podman = ContainerRuntime::Podman;
        assert_ne!(docker, podman);
        assert_eq!(docker.command(), "docker");
        assert_eq!(podman.command(), "podman");
    }

    #[test]
    fn test_truncate_output_one_char_over() {
        let output = "123456";
        let result = truncate_output(output, 5);
        assert!(result.contains("truncated"));
        assert!(result.contains("6 total chars"));
    }

    #[test]
    fn test_container_info_all_fields() {
        let info = ContainerInfo {
            id: "container123".to_string(),
            image: "myimage:v1".to_string(),
            command: "/bin/bash".to_string(),
            created: "2025-01-01 10:00:00".to_string(),
            status: "Up 2 hours".to_string(),
            ports: "0.0.0.0:8080->80/tcp".to_string(),
            names: "my_container".to_string(),
        };
        assert_eq!(info.id, "container123");
        assert_eq!(info.image, "myimage:v1");
        assert_eq!(info.command, "/bin/bash");
        assert_eq!(info.created, "2025-01-01 10:00:00");
        assert_eq!(info.status, "Up 2 hours");
        assert_eq!(info.ports, "0.0.0.0:8080->80/tcp");
        assert_eq!(info.names, "my_container");
    }

    #[test]
    fn test_image_info_all_fields() {
        let info = ImageInfo {
            id: "sha256:abcdef123456".to_string(),
            repository: "myregistry/myapp".to_string(),
            tag: "v2.0.0".to_string(),
            created: "3 days ago".to_string(),
            size: "250MB".to_string(),
        };
        assert_eq!(info.id, "sha256:abcdef123456");
        assert_eq!(info.repository, "myregistry/myapp");
        assert_eq!(info.tag, "v2.0.0");
        assert_eq!(info.created, "3 days ago");
        assert_eq!(info.size, "250MB");
    }

    #[test]
    fn test_parse_build_output_no_sha256_prefix() {
        let stdout = "Building...\nSuccessfully built finalimage123";
        let result = parse_build_output(stdout, "");
        assert_eq!(result, Some("finalimage123".to_string()));
    }

    #[test]
    fn test_container_run_schema_all_options() {
        let tool = ContainerRun;
        let schema = tool.schema();
        let props = &schema["properties"];
        // Verify all expected properties exist
        assert!(props.get("image").is_some());
        assert!(props.get("name").is_some());
        assert!(props.get("command").is_some());
        assert!(props.get("ports").is_some());
        assert!(props.get("volumes").is_some());
        assert!(props.get("env").is_some());
        assert!(props.get("detach").is_some());
        assert!(props.get("rm").is_some());
        assert!(props.get("network").is_some());
        assert!(props.get("workdir").is_some());
        assert!(props.get("runtime").is_some());
    }

    #[test]
    fn test_container_exec_schema_all_options() {
        let tool = ContainerExec;
        let schema = tool.schema();
        let props = &schema["properties"];
        assert!(props.get("container").is_some());
        assert!(props.get("command").is_some());
        assert!(props.get("workdir").is_some());
        assert!(props.get("env").is_some());
        assert!(props.get("user").is_some());
        assert!(props.get("runtime").is_some());
    }

    #[test]
    fn test_container_build_schema_all_options() {
        let tool = ContainerBuild;
        let schema = tool.schema();
        let props = &schema["properties"];
        assert!(props.get("tag").is_some());
        assert!(props.get("path").is_some());
        assert!(props.get("dockerfile").is_some());
        assert!(props.get("build_args").is_some());
        assert!(props.get("no_cache").is_some());
        assert!(props.get("target").is_some());
        assert!(props.get("runtime").is_some());
    }

    #[test]
    fn test_compose_up_schema_all_options() {
        let tool = ComposeUp;
        let schema = tool.schema();
        let props = &schema["properties"];
        assert!(props.get("path").is_some());
        assert!(props.get("file").is_some());
        assert!(props.get("services").is_some());
        assert!(props.get("detach").is_some());
        assert!(props.get("build").is_some());
        assert!(props.get("runtime").is_some());
    }

    #[test]
    fn test_compose_down_schema_all_options() {
        let tool = ComposeDown;
        let schema = tool.schema();
        let props = &schema["properties"];
        assert!(props.get("path").is_some());
        assert!(props.get("file").is_some());
        assert!(props.get("volumes").is_some());
        assert!(props.get("rmi").is_some());
        assert!(props.get("runtime").is_some());
    }

    #[test]
    fn test_all_tools_have_properties() {
        let tools: Vec<Box<dyn Tool + Send + Sync>> = vec![
            Box::new(ContainerRun),
            Box::new(ContainerStop),
            Box::new(ContainerList),
            Box::new(ContainerLogs),
            Box::new(ContainerExec),
            Box::new(ContainerBuild),
            Box::new(ContainerImages),
            Box::new(ContainerPull),
            Box::new(ContainerRemove),
            Box::new(ComposeUp),
            Box::new(ComposeDown),
        ];
        for tool in tools {
            let schema = tool.schema();
            assert!(
                schema.get("properties").is_some(),
                "Tool {} missing properties",
                tool.name()
            );
        }
    }

    #[test]
    fn test_truncate_output_newlines() {
        let output = "line1\nline2\nline3\nline4\nline5";
        let result = truncate_output(output, 10);
        assert!(result.contains("truncated"));
    }

    #[test]
    fn test_parse_build_output_sha256_no_suffix() {
        let stderr = "writing image sha256:abc123";
        let result = parse_build_output("", stderr);
        assert_eq!(result, Some("abc123".to_string()));
    }

    #[test]
    fn test_container_info_empty_ports() {
        let info = ContainerInfo {
            id: "abc".to_string(),
            image: "test".to_string(),
            command: "sh".to_string(),
            created: "now".to_string(),
            status: "exited".to_string(),
            ports: "".to_string(),
            names: "test".to_string(),
        };
        assert!(info.ports.is_empty());
    }

    #[test]
    fn test_image_info_none_tag() {
        let info = ImageInfo {
            id: "sha".to_string(),
            repository: "test".to_string(),
            tag: "<none>".to_string(),
            created: "now".to_string(),
            size: "0B".to_string(),
        };
        assert_eq!(info.tag, "<none>");
    }

    #[test]
    fn test_runtime_eq_reflexive() {
        let runtime = ContainerRuntime::Docker;
        assert_eq!(runtime, runtime);
    }

    #[test]
    fn test_runtime_ne_different() {
        assert!(ContainerRuntime::Docker != ContainerRuntime::Podman);
    }

    #[test]
    fn test_container_stop_schema_has_container() {
        let tool = ContainerStop;
        let schema = tool.schema();
        let required = schema["required"].as_array().unwrap();
        assert!(required.contains(&json!("container")));
    }

    #[test]
    fn test_container_logs_schema_has_container() {
        let tool = ContainerLogs;
        let schema = tool.schema();
        let required = schema["required"].as_array().unwrap();
        assert!(required.contains(&json!("container")));
    }

    #[test]
    fn test_container_exec_schema_has_required() {
        let tool = ContainerExec;
        let schema = tool.schema();
        let required = schema["required"].as_array().unwrap();
        assert!(required.contains(&json!("container")));
        assert!(required.contains(&json!("command")));
    }

    #[test]
    fn test_container_build_schema_has_tag() {
        let tool = ContainerBuild;
        let schema = tool.schema();
        let required = schema["required"].as_array().unwrap();
        assert!(required.contains(&json!("tag")));
    }

    #[test]
    fn test_container_remove_schema_has_container() {
        let tool = ContainerRemove;
        let schema = tool.schema();
        let required = schema["required"].as_array().unwrap();
        assert!(required.contains(&json!("container")));
    }

    #[test]
    fn test_truncate_output_multibyte_boundary() {
        // Test truncation near multibyte char boundary
        let output = "a".repeat(100);
        let result = truncate_output(&output, 50);
        assert!(result.starts_with("aaaaa"));
        assert!(result.contains("truncated"));
    }

    #[test]
    fn test_parse_build_output_both_sources() {
        // When both stdout and stderr have relevant content
        let stdout = "Building layer 1";
        let stderr = "writing image sha256:finalid123 done";
        let result = parse_build_output(stdout, stderr);
        assert_eq!(result, Some("finalid123".to_string()));
    }

    #[test]
    fn test_container_run_schema_type() {
        let tool = ContainerRun;
        let schema = tool.schema();
        assert_eq!(schema["type"], "object");
    }

    #[test]
    fn test_container_images_schema_filter_type() {
        let tool = ContainerImages;
        let schema = tool.schema();
        let filter = &schema["properties"]["filter"];
        assert_eq!(filter["type"], "string");
    }

    #[test]
    fn test_all_schemas_have_type() {
        let tools: Vec<Box<dyn Tool + Send + Sync>> = vec![
            Box::new(ContainerRun),
            Box::new(ContainerStop),
            Box::new(ContainerList),
            Box::new(ContainerLogs),
            Box::new(ContainerExec),
            Box::new(ContainerBuild),
            Box::new(ContainerImages),
            Box::new(ContainerPull),
            Box::new(ContainerRemove),
            Box::new(ComposeUp),
            Box::new(ComposeDown),
        ];
        for tool in tools {
            assert_eq!(
                tool.schema()["type"],
                "object",
                "Tool {} missing type",
                tool.name()
            );
        }
    }
}
