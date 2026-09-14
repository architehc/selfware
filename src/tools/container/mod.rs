//! Container Tools (Docker & Podman)
//!
//! Tools for managing containers with automatic runtime detection.
//! Supports both Docker and Podman (CLI-compatible).

mod runtime;
mod tools;
mod validation;

#[cfg(test)]
mod tests;

// Re-export public types
pub use runtime::{get_runtime, ContainerRuntime};
pub use tools::{
    security_flags, ComposeDown, ComposeUp, ContainerBuild, ContainerExec, ContainerImages,
    ContainerList, ContainerLogs, ContainerPull, ContainerRemove, ContainerRun, ContainerStop,
};
pub use validation::{
    is_valid_memory, is_valid_port, is_valid_user, validate_port_mapping, validate_volume_spec,
};
