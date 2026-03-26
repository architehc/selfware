//! Container Runtime Detection
//!
//! Detects and manages Docker and Podman container runtimes.

use anyhow::Result;
use std::process::Stdio;
use tokio::process::Command;

/// Detected container runtime
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContainerRuntime {
    Docker,
    Podman,
}

impl ContainerRuntime {
    pub fn command(&self) -> &'static str {
        match self {
            ContainerRuntime::Docker => "docker",
            ContainerRuntime::Podman => "podman",
        }
    }
}

/// Detect available container runtime (prefers Docker, falls back to Podman)
pub async fn detect_runtime() -> Result<ContainerRuntime> {
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
pub async fn get_runtime(preferred: Option<&str>) -> Result<ContainerRuntime> {
    match preferred {
        Some("docker") => Ok(ContainerRuntime::Docker),
        Some("podman") => Ok(ContainerRuntime::Podman),
        _ => detect_runtime().await,
    }
}
