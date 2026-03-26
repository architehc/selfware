//! Container Tool Implementations
//!
//! Individual tool implementations for container management.

use anyhow::{Context, Result};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::process::Stdio;
use tokio::process::Command;

use super::runtime::{get_runtime, ContainerRuntime};
use super::validation::{validate_port_mapping, validate_volume_spec};
use crate::tools::Tool;

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

        const FORBIDDEN_CHARS: &[char] = &[';', '&', '|', '`', '$', '(', ')', '<', '>'];
        for arg in &command {
            if arg.chars().any(|c| FORBIDDEN_CHARS.contains(&c)) {
                anyhow::bail!("Blocked forbidden metacharacter in container command argument.");
            }
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
    crate::tools::truncate_output(output, max_len)
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
