//! Container Management System
//!
//! Docker/Podman integration: build images, run containers, exec commands,
//! manage volumes, compose support.
//!
//! # Features
//!
//! - Container runtime abstraction (Docker/Podman)
//! - Image management (build, pull, push, list)
//! - Container lifecycle (run, stop, start, remove)
//! - Volume and network management
//! - Docker Compose support

use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

/// Container runtime type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeType {
    /// Docker container runtime
    Docker,
    /// Podman container runtime
    Podman,
    /// Container runtime auto-detection
    Auto,
}

impl RuntimeType {
    /// Get the command name for this runtime
    pub fn command(&self) -> &'static str {
        match self {
            RuntimeType::Docker => "docker",
            RuntimeType::Podman => "podman",
            RuntimeType::Auto => "docker", // Default to docker
        }
    }
}

/// Container status
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContainerStatus {
    /// Container is running
    Running,
    /// Container is paused
    Paused,
    /// Container is stopped
    Stopped,
    /// Container is being created
    Creating,
    /// Container is being removed
    Removing,
    /// Container has exited
    Exited,
    /// Container is dead
    Dead,
    /// Unknown status
    Unknown,
}

impl ContainerStatus {
    /// Parse status from string
    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "running" => ContainerStatus::Running,
            "paused" => ContainerStatus::Paused,
            "stopped" => ContainerStatus::Stopped,
            "created" | "creating" => ContainerStatus::Creating,
            "removing" => ContainerStatus::Removing,
            "exited" => ContainerStatus::Exited,
            "dead" => ContainerStatus::Dead,
            _ => ContainerStatus::Unknown,
        }
    }
}

/// Container information
#[derive(Debug, Clone)]
pub struct Container {
    /// Container ID
    pub id: String,
    /// Container name
    pub name: String,
    /// Image used
    pub image: String,
    /// Current status
    pub status: ContainerStatus,
    /// Ports mapping (host:container)
    pub ports: Vec<PortMapping>,
    /// Environment variables
    pub env: HashMap<String, String>,
    /// Volume mounts
    pub volumes: Vec<VolumeMount>,
    /// Labels
    pub labels: HashMap<String, String>,
    /// Created timestamp
    pub created_at: u64,
    /// Command
    pub command: Option<String>,
}

impl Container {
    /// Create a new container definition
    pub fn new(name: &str, image: &str) -> Self {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        Self {
            id: String::new(),
            name: name.to_string(),
            image: image.to_string(),
            status: ContainerStatus::Creating,
            ports: Vec::new(),
            env: HashMap::new(),
            volumes: Vec::new(),
            labels: HashMap::new(),
            created_at: now,
            command: None,
        }
    }

    /// Add port mapping
    pub fn with_port(mut self, host: u16, container: u16) -> Self {
        self.ports.push(PortMapping {
            host_port: host,
            container_port: container,
            protocol: Protocol::Tcp,
            host_ip: None,
        });
        self
    }

    /// Add environment variable
    pub fn with_env(mut self, key: &str, value: &str) -> Self {
        self.env.insert(key.to_string(), value.to_string());
        self
    }

    /// Add volume mount
    pub fn with_volume(mut self, host_path: &str, container_path: &str) -> Self {
        self.volumes.push(VolumeMount {
            source: host_path.to_string(),
            target: container_path.to_string(),
            read_only: false,
            mount_type: MountType::Bind,
        });
        self
    }

    /// Add label
    pub fn with_label(mut self, key: &str, value: &str) -> Self {
        self.labels.insert(key.to_string(), value.to_string());
        self
    }

    /// Set command
    pub fn with_command(mut self, cmd: &str) -> Self {
        self.command = Some(cmd.to_string());
        self
    }

    /// Check if container is running
    pub fn is_running(&self) -> bool {
        self.status == ContainerStatus::Running
    }
}

/// Port mapping
#[derive(Debug, Clone)]
pub struct PortMapping {
    /// Host port
    pub host_port: u16,
    /// Container port
    pub container_port: u16,
    /// Protocol
    pub protocol: Protocol,
    /// Host IP to bind
    pub host_ip: Option<String>,
}

impl PortMapping {
    /// Format as docker port argument
    pub fn to_docker_arg(&self) -> String {
        let proto = match self.protocol {
            Protocol::Tcp => "",
            Protocol::Udp => "/udp",
        };
        if let Some(ref ip) = self.host_ip {
            format!("{}:{}:{}{}", ip, self.host_port, self.container_port, proto)
        } else {
            format!("{}:{}{}", self.host_port, self.container_port, proto)
        }
    }
}

/// Network protocol
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Protocol {
    Tcp,
    Udp,
}

/// Volume mount
#[derive(Debug, Clone)]
pub struct VolumeMount {
    /// Source path or volume name
    pub source: String,
    /// Target path in container
    pub target: String,
    /// Read-only mount
    pub read_only: bool,
    /// Mount type
    pub mount_type: MountType,
}

impl VolumeMount {
    /// Format as docker volume argument
    pub fn to_docker_arg(&self) -> String {
        let ro = if self.read_only { ":ro" } else { "" };
        format!("{}:{}{}", self.source, self.target, ro)
    }
}

/// Mount type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MountType {
    /// Bind mount from host
    Bind,
    /// Named volume
    Volume,
    /// tmpfs mount
    Tmpfs,
}

/// Image information
#[derive(Debug, Clone)]
pub struct Image {
    /// Image ID
    pub id: String,
    /// Repository name
    pub repository: String,
    /// Image tag
    pub tag: String,
    /// Image size in bytes
    pub size: u64,
    /// Created timestamp
    pub created_at: u64,
    /// Labels
    pub labels: HashMap<String, String>,
}

impl Image {
    /// Get full image reference
    pub fn reference(&self) -> String {
        format!("{}:{}", self.repository, self.tag)
    }

    /// Parse image reference
    pub fn parse_reference(reference: &str) -> (String, String) {
        if let Some((repo, tag)) = reference.rsplit_once(':') {
            // Check if this is actually a port number (e.g., localhost:5000/image)
            if tag.contains('/') || tag.parse::<u16>().is_ok() {
                (reference.to_string(), "latest".to_string())
            } else {
                (repo.to_string(), tag.to_string())
            }
        } else {
            (reference.to_string(), "latest".to_string())
        }
    }
}

/// Volume information
#[derive(Debug, Clone)]
pub struct Volume {
    /// Volume name
    pub name: String,
    /// Driver
    pub driver: String,
    /// Mount point
    pub mountpoint: Option<String>,
    /// Labels
    pub labels: HashMap<String, String>,
    /// Created timestamp
    pub created_at: u64,
}

impl Volume {
    /// Create a new volume definition
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            driver: "local".to_string(),
            mountpoint: None,
            labels: HashMap::new(),
            created_at: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
        }
    }

    /// Set driver
    pub fn with_driver(mut self, driver: &str) -> Self {
        self.driver = driver.to_string();
        self
    }
}

/// Network information
#[derive(Debug, Clone)]
pub struct Network {
    /// Network ID
    pub id: String,
    /// Network name
    pub name: String,
    /// Network driver
    pub driver: String,
    /// Subnet
    pub subnet: Option<String>,
    /// Gateway
    pub gateway: Option<String>,
    /// Labels
    pub labels: HashMap<String, String>,
}

impl Network {
    /// Create a new network definition
    pub fn new(name: &str) -> Self {
        Self {
            id: String::new(),
            name: name.to_string(),
            driver: "bridge".to_string(),
            subnet: None,
            gateway: None,
            labels: HashMap::new(),
        }
    }

    /// Set driver
    pub fn with_driver(mut self, driver: &str) -> Self {
        self.driver = driver.to_string();
        self
    }

    /// Set subnet
    pub fn with_subnet(mut self, subnet: &str) -> Self {
        self.subnet = Some(subnet.to_string());
        self
    }
}

/// Build context for image builds
#[derive(Debug, Clone)]
pub struct BuildContext {
    /// Path to build context
    pub context_path: String,
    /// Dockerfile path (relative to context)
    pub dockerfile: Option<String>,
    /// Image tag
    pub tag: Option<String>,
    /// Build arguments
    pub build_args: HashMap<String, String>,
    /// Target stage (multi-stage builds)
    pub target: Option<String>,
    /// No cache
    pub no_cache: bool,
    /// Pull latest base images
    pub pull: bool,
}

impl BuildContext {
    /// Create a new build context
    pub fn new(context_path: &str) -> Self {
        Self {
            context_path: context_path.to_string(),
            dockerfile: None,
            tag: None,
            build_args: HashMap::new(),
            target: None,
            no_cache: false,
            pull: false,
        }
    }

    /// Set Dockerfile path
    pub fn with_dockerfile(mut self, path: &str) -> Self {
        self.dockerfile = Some(path.to_string());
        self
    }

    /// Set image tag
    pub fn with_tag(mut self, tag: &str) -> Self {
        self.tag = Some(tag.to_string());
        self
    }

    /// Add build argument
    pub fn with_arg(mut self, key: &str, value: &str) -> Self {
        self.build_args.insert(key.to_string(), value.to_string());
        self
    }

    /// Set target stage
    pub fn with_target(mut self, target: &str) -> Self {
        self.target = Some(target.to_string());
        self
    }

    /// Enable no-cache
    pub fn no_cache(mut self) -> Self {
        self.no_cache = true;
        self
    }

    /// Enable pull
    pub fn pull_latest(mut self) -> Self {
        self.pull = true;
        self
    }
}

/// Exec configuration
#[derive(Debug, Clone)]
pub struct ExecConfig {
    /// Command to execute
    pub command: Vec<String>,
    /// Working directory
    pub workdir: Option<String>,
    /// Environment variables
    pub env: HashMap<String, String>,
    /// Run as user
    pub user: Option<String>,
    /// Allocate TTY
    pub tty: bool,
    /// Run interactively
    pub interactive: bool,
    /// Detach
    pub detach: bool,
}

impl ExecConfig {
    /// Create a new exec config
    pub fn new(command: &[&str]) -> Self {
        Self {
            command: command.iter().map(|s| s.to_string()).collect(),
            workdir: None,
            env: HashMap::new(),
            user: None,
            tty: false,
            interactive: false,
            detach: false,
        }
    }

    /// Set working directory
    pub fn in_dir(mut self, dir: &str) -> Self {
        self.workdir = Some(dir.to_string());
        self
    }

    /// Add environment variable
    pub fn with_env(mut self, key: &str, value: &str) -> Self {
        self.env.insert(key.to_string(), value.to_string());
        self
    }

    /// Run as user
    pub fn as_user(mut self, user: &str) -> Self {
        self.user = Some(user.to_string());
        self
    }

    /// Enable TTY
    pub fn with_tty(mut self) -> Self {
        self.tty = true;
        self
    }

    /// Enable interactive mode
    pub fn interactive(mut self) -> Self {
        self.interactive = true;
        self
    }
}

/// Command result
#[derive(Debug, Clone)]
pub struct CommandResult {
    /// Exit code
    pub exit_code: i32,
    /// Standard output
    pub stdout: String,
    /// Standard error
    pub stderr: String,
    /// Execution time in milliseconds
    pub duration_ms: u64,
}

impl CommandResult {
    /// Check if command succeeded
    pub fn success(&self) -> bool {
        self.exit_code == 0
    }

    /// Create a success result
    pub fn ok(stdout: &str) -> Self {
        Self {
            exit_code: 0,
            stdout: stdout.to_string(),
            stderr: String::new(),
            duration_ms: 0,
        }
    }

    /// Create an error result
    pub fn error(code: i32, stderr: &str) -> Self {
        Self {
            exit_code: code,
            stdout: String::new(),
            stderr: stderr.to_string(),
            duration_ms: 0,
        }
    }
}

/// Container manager
#[derive(Debug)]
pub struct ContainerManager {
    /// Runtime type
    pub runtime: RuntimeType,
    /// Detected runtime command
    pub runtime_command: String,
    /// Container cache
    containers: HashMap<String, Container>,
    /// Image cache
    images: HashMap<String, Image>,
    /// Command history
    history: Vec<String>,
}

impl Default for ContainerManager {
    fn default() -> Self {
        Self::new(RuntimeType::Auto)
    }
}

impl ContainerManager {
    /// Create a new container manager
    pub fn new(runtime: RuntimeType) -> Self {
        let runtime_command = match runtime {
            RuntimeType::Auto => Self::detect_runtime(),
            _ => runtime.command().to_string(),
        };

        Self {
            runtime,
            runtime_command,
            containers: HashMap::new(),
            images: HashMap::new(),
            history: Vec::new(),
        }
    }

    /// Detect available runtime
    fn detect_runtime() -> String {
        // In real implementation, would check for docker/podman
        // For now, default to docker
        "docker".to_string()
    }

    /// Build command line for container run
    pub fn build_run_command(&self, container: &Container) -> Vec<String> {
        let mut args = vec![
            self.runtime_command.clone(),
            "run".to_string(),
            "-d".to_string(),
            "--name".to_string(),
            container.name.clone(),
        ];

        // Add ports
        for port in &container.ports {
            args.push("-p".to_string());
            args.push(port.to_docker_arg());
        }

        // Add environment
        for (key, value) in &container.env {
            args.push("-e".to_string());
            args.push(format!("{}={}", key, value));
        }

        // Add volumes
        for volume in &container.volumes {
            args.push("-v".to_string());
            args.push(volume.to_docker_arg());
        }

        // Add labels
        for (key, value) in &container.labels {
            args.push("-l".to_string());
            args.push(format!("{}={}", key, value));
        }

        // Add image
        args.push(container.image.clone());

        // Add command if specified — use shell-aware splitting to respect quotes
        if let Some(ref cmd) = container.command {
            match shlex::split(cmd) {
                Some(parts) => args.extend(parts),
                None => {
                    // Unbalanced quotes — fall back to whitespace split with a warning
                    tracing::warn!("Container command has unbalanced quotes, falling back to whitespace split: {}", cmd);
                    args.extend(cmd.split_whitespace().map(String::from));
                }
            }
        }

        args
    }

    /// Build command line for image build
    pub fn build_build_command(&self, context: &BuildContext) -> Vec<String> {
        let mut args = vec![self.runtime_command.clone(), "build".to_string()];

        if let Some(ref tag) = context.tag {
            args.push("-t".to_string());
            args.push(tag.clone());
        }

        if let Some(ref dockerfile) = context.dockerfile {
            args.push("-f".to_string());
            args.push(dockerfile.clone());
        }

        if let Some(ref target) = context.target {
            args.push("--target".to_string());
            args.push(target.clone());
        }

        for (key, value) in &context.build_args {
            args.push("--build-arg".to_string());
            args.push(format!("{}={}", key, value));
        }

        if context.no_cache {
            args.push("--no-cache".to_string());
        }

        if context.pull {
            args.push("--pull".to_string());
        }

        args.push(context.context_path.clone());

        args
    }

    /// Build command line for exec
    pub fn build_exec_command(&self, container_id: &str, config: &ExecConfig) -> Vec<String> {
        let mut args = vec![self.runtime_command.clone(), "exec".to_string()];

        if config.interactive {
            args.push("-i".to_string());
        }

        if config.tty {
            args.push("-t".to_string());
        }

        if config.detach {
            args.push("-d".to_string());
        }

        if let Some(ref workdir) = config.workdir {
            args.push("-w".to_string());
            args.push(workdir.clone());
        }

        if let Some(ref user) = config.user {
            args.push("-u".to_string());
            args.push(user.clone());
        }

        for (key, value) in &config.env {
            args.push("-e".to_string());
            args.push(format!("{}={}", key, value));
        }

        args.push(container_id.to_string());
        args.extend(config.command.clone());

        args
    }

    /// Simulate running a container
    pub fn run(&mut self, container: Container) -> CommandResult {
        let cmd = self.build_run_command(&container);
        self.history.push(cmd.join(" "));

        // Simulate container creation
        let id = format!("container_{}", self.containers.len());
        let mut new_container = container;
        new_container.id = id.clone();
        new_container.status = ContainerStatus::Running;

        self.containers.insert(id.clone(), new_container);

        CommandResult::ok(&id)
    }

    /// Simulate stopping a container
    pub fn stop(&mut self, container_id: &str) -> CommandResult {
        self.history
            .push(format!("{} stop {}", self.runtime_command, container_id));

        if let Some(container) = self.containers.get_mut(container_id) {
            container.status = ContainerStatus::Stopped;
            CommandResult::ok(container_id)
        } else {
            CommandResult::error(1, &format!("Container not found: {}", container_id))
        }
    }

    /// Simulate starting a container
    pub fn start(&mut self, container_id: &str) -> CommandResult {
        self.history
            .push(format!("{} start {}", self.runtime_command, container_id));

        if let Some(container) = self.containers.get_mut(container_id) {
            container.status = ContainerStatus::Running;
            CommandResult::ok(container_id)
        } else {
            CommandResult::error(1, &format!("Container not found: {}", container_id))
        }
    }

    /// Simulate removing a container
    pub fn remove(&mut self, container_id: &str, force: bool) -> CommandResult {
        let force_flag = if force { " -f" } else { "" };
        self.history.push(format!(
            "{} rm{} {}",
            self.runtime_command, force_flag, container_id
        ));

        if self.containers.remove(container_id).is_some() {
            CommandResult::ok(container_id)
        } else {
            CommandResult::error(1, &format!("Container not found: {}", container_id))
        }
    }

    /// Simulate executing command in container
    pub fn exec(&mut self, container_id: &str, config: ExecConfig) -> CommandResult {
        let cmd = self.build_exec_command(container_id, &config);
        self.history.push(cmd.join(" "));

        if self.containers.contains_key(container_id) {
            CommandResult::ok(&format!("[Executed: {}]", config.command.join(" ")))
        } else {
            CommandResult::error(1, &format!("Container not found: {}", container_id))
        }
    }

    /// Simulate getting logs
    pub fn logs(&self, container_id: &str, tail: Option<u32>) -> CommandResult {
        let tail_arg = tail.map(|n| format!(" --tail {}", n)).unwrap_or_default();
        let _cmd = format!("{} logs{} {}", self.runtime_command, tail_arg, container_id);

        if self.containers.contains_key(container_id) {
            CommandResult::ok("[Container logs would appear here]")
        } else {
            CommandResult::error(1, &format!("Container not found: {}", container_id))
        }
    }

    /// Simulate building an image
    pub fn build(&mut self, context: BuildContext) -> CommandResult {
        let cmd = self.build_build_command(&context);
        self.history.push(cmd.join(" "));

        let image_id = format!("sha256:{:016x}", self.images.len());
        if let Some(tag) = context.tag {
            let (repo, tag_name) = Image::parse_reference(&tag);
            let image = Image {
                id: image_id.clone(),
                repository: repo,
                tag: tag_name,
                size: 100_000_000, // 100MB placeholder
                created_at: SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs(),
                labels: HashMap::new(),
            };
            self.images.insert(image.reference(), image);
        }

        CommandResult::ok(&image_id)
    }

    /// Simulate pulling an image
    pub fn pull(&mut self, reference: &str) -> CommandResult {
        self.history
            .push(format!("{} pull {}", self.runtime_command, reference));

        let (repo, tag) = Image::parse_reference(reference);
        let image_id = format!("sha256:{:016x}", self.images.len());
        let image = Image {
            id: image_id.clone(),
            repository: repo,
            tag,
            size: 50_000_000,
            created_at: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            labels: HashMap::new(),
        };

        let ref_key = image.reference();
        self.images.insert(ref_key, image);

        CommandResult::ok(&image_id)
    }

    /// Simulate pushing an image
    pub fn push(&self, reference: &str) -> CommandResult {
        let _cmd = format!("{} push {}", self.runtime_command, reference);

        if self.images.contains_key(reference) {
            CommandResult::ok(reference)
        } else {
            CommandResult::error(1, &format!("Image not found: {}", reference))
        }
    }

    /// List containers
    pub fn list_containers(&self, all: bool) -> Vec<&Container> {
        self.containers
            .values()
            .filter(|c| all || c.status == ContainerStatus::Running)
            .collect()
    }

    /// List images
    pub fn list_images(&self) -> Vec<&Image> {
        self.images.values().collect()
    }

    /// Get container by ID or name
    pub fn get_container(&self, id_or_name: &str) -> Option<&Container> {
        self.containers
            .get(id_or_name)
            .or_else(|| self.containers.values().find(|c| c.name == id_or_name))
    }

    /// Get command history
    pub fn get_history(&self) -> &[String] {
        &self.history
    }

    /// Clear history
    pub fn clear_history(&mut self) {
        self.history.clear();
    }
}

/// Docker Compose service
#[derive(Debug, Clone)]
pub struct ComposeService {
    /// Service name
    pub name: String,
    /// Image
    pub image: Option<String>,
    /// Build context
    pub build: Option<String>,
    /// Ports
    pub ports: Vec<String>,
    /// Environment variables
    pub environment: HashMap<String, String>,
    /// Volumes
    pub volumes: Vec<String>,
    /// Dependencies
    pub depends_on: Vec<String>,
    /// Command
    pub command: Option<String>,
    /// Restart policy
    pub restart: Option<String>,
    /// Networks
    pub networks: Vec<String>,
}

impl ComposeService {
    /// Create a new service
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            image: None,
            build: None,
            ports: Vec::new(),
            environment: HashMap::new(),
            volumes: Vec::new(),
            depends_on: Vec::new(),
            command: None,
            restart: None,
            networks: Vec::new(),
        }
    }

    /// Set image
    pub fn with_image(mut self, image: &str) -> Self {
        self.image = Some(image.to_string());
        self
    }

    /// Set build context
    pub fn with_build(mut self, context: &str) -> Self {
        self.build = Some(context.to_string());
        self
    }

    /// Add port
    pub fn with_port(mut self, port: &str) -> Self {
        self.ports.push(port.to_string());
        self
    }

    /// Add environment variable
    pub fn with_env(mut self, key: &str, value: &str) -> Self {
        self.environment.insert(key.to_string(), value.to_string());
        self
    }

    /// Add volume
    pub fn with_volume(mut self, volume: &str) -> Self {
        self.volumes.push(volume.to_string());
        self
    }

    /// Add dependency
    pub fn depends(mut self, service: &str) -> Self {
        self.depends_on.push(service.to_string());
        self
    }

    /// Set restart policy
    pub fn with_restart(mut self, policy: &str) -> Self {
        self.restart = Some(policy.to_string());
        self
    }
}

/// Docker Compose file
#[derive(Debug, Clone)]
pub struct ComposeFile {
    /// Compose version
    pub version: String,
    /// Services
    pub services: HashMap<String, ComposeService>,
    /// Volumes
    pub volumes: HashMap<String, ComposeVolume>,
    /// Networks
    pub networks: HashMap<String, ComposeNetwork>,
}

/// Compose volume definition
#[derive(Debug, Clone)]
pub struct ComposeVolume {
    /// Driver
    pub driver: Option<String>,
    /// External
    pub external: bool,
}

/// Compose network definition
#[derive(Debug, Clone)]
pub struct ComposeNetwork {
    /// Driver
    pub driver: Option<String>,
    /// External
    pub external: bool,
}

impl ComposeFile {
    /// Create a new compose file
    pub fn new() -> Self {
        Self {
            version: "3.8".to_string(),
            services: HashMap::new(),
            volumes: HashMap::new(),
            networks: HashMap::new(),
        }
    }

    /// Add a service
    pub fn add_service(&mut self, service: ComposeService) {
        self.services.insert(service.name.clone(), service);
    }

    /// Add a volume
    pub fn add_volume(&mut self, name: &str) {
        self.volumes.insert(
            name.to_string(),
            ComposeVolume {
                driver: None,
                external: false,
            },
        );
    }

    /// Add a network
    pub fn add_network(&mut self, name: &str) {
        self.networks.insert(
            name.to_string(),
            ComposeNetwork {
                driver: None,
                external: false,
            },
        );
    }

    /// Generate YAML representation
    pub fn to_yaml(&self) -> String {
        let mut yaml = format!("version: \"{}\"\n\nservices:\n", self.version);

        for (name, service) in &self.services {
            yaml.push_str(&format!("  {}:\n", name));

            if let Some(ref image) = service.image {
                yaml.push_str(&format!("    image: {}\n", image));
            }

            if let Some(ref build) = service.build {
                yaml.push_str(&format!("    build: {}\n", build));
            }

            if !service.ports.is_empty() {
                yaml.push_str("    ports:\n");
                for port in &service.ports {
                    yaml.push_str(&format!("      - \"{}\"\n", port));
                }
            }

            if !service.environment.is_empty() {
                yaml.push_str("    environment:\n");
                for (key, value) in &service.environment {
                    yaml.push_str(&format!("      {}: {}\n", key, value));
                }
            }

            if !service.volumes.is_empty() {
                yaml.push_str("    volumes:\n");
                for vol in &service.volumes {
                    yaml.push_str(&format!("      - {}\n", vol));
                }
            }

            if !service.depends_on.is_empty() {
                yaml.push_str("    depends_on:\n");
                for dep in &service.depends_on {
                    yaml.push_str(&format!("      - {}\n", dep));
                }
            }

            if let Some(ref restart) = service.restart {
                yaml.push_str(&format!("    restart: {}\n", restart));
            }

            if let Some(ref command) = service.command {
                yaml.push_str(&format!("    command: {}\n", command));
            }
        }

        if !self.volumes.is_empty() {
            yaml.push_str("\nvolumes:\n");
            for name in self.volumes.keys() {
                yaml.push_str(&format!("  {}:\n", name));
            }
        }

        if !self.networks.is_empty() {
            yaml.push_str("\nnetworks:\n");
            for name in self.networks.keys() {
                yaml.push_str(&format!("  {}:\n", name));
            }
        }

        yaml
    }

    /// Parse simple YAML (basic implementation)
    pub fn parse_yaml(content: &str) -> Option<Self> {
        let mut compose = Self::new();
        let mut current_service: Option<String> = None;
        let mut in_services = false;

        for line in content.lines() {
            let trimmed = line.trim();

            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }

            if trimmed.starts_with("version:") {
                if let Some(v) = trimmed.strip_prefix("version:") {
                    compose.version = v.trim().trim_matches('"').to_string();
                }
            } else if trimmed == "services:" {
                in_services = true;
            } else if in_services {
                // Count leading spaces
                let indent = line.len() - line.trim_start().len();

                if indent == 2 && trimmed.ends_with(':') {
                    // Service name
                    let name = trimmed.trim_end_matches(':');
                    current_service = Some(name.to_string());
                    compose
                        .services
                        .insert(name.to_string(), ComposeService::new(name));
                } else if indent == 4 && current_service.is_some() {
                    // Service property
                    let service_name = current_service.as_ref().unwrap();
                    if let Some(service) = compose.services.get_mut(service_name) {
                        if let Some((key, value)) = trimmed.split_once(':') {
                            let value = value.trim().trim_matches('"');
                            match key.trim() {
                                "image" => service.image = Some(value.to_string()),
                                "build" => service.build = Some(value.to_string()),
                                "restart" => service.restart = Some(value.to_string()),
                                "command" => service.command = Some(value.to_string()),
                                _ => {}
                            }
                        }
                    }
                }
            }
        }

        Some(compose)
    }
}

impl Default for ComposeFile {
    fn default() -> Self {
        Self::new()
    }
}

/// Compose manager
#[derive(Debug)]
pub struct ComposeManager {
    /// Container manager
    pub container_manager: ContainerManager,
    /// Loaded compose files
    pub compose_files: HashMap<String, ComposeFile>,
    /// Running stacks
    pub stacks: HashMap<String, Vec<String>>,
}

impl Default for ComposeManager {
    fn default() -> Self {
        Self::new()
    }
}

impl ComposeManager {
    /// Create a new compose manager
    pub fn new() -> Self {
        Self {
            container_manager: ContainerManager::default(),
            compose_files: HashMap::new(),
            stacks: HashMap::new(),
        }
    }

    /// Load a compose file
    pub fn load(&mut self, name: &str, content: &str) -> bool {
        if let Some(compose) = ComposeFile::parse_yaml(content) {
            self.compose_files.insert(name.to_string(), compose);
            true
        } else {
            false
        }
    }

    /// Up a stack
    pub fn up(&mut self, stack_name: &str) -> CommandResult {
        let compose = match self.compose_files.get(stack_name) {
            Some(c) => c.clone(),
            None => return CommandResult::error(1, "Compose file not found"),
        };

        let mut container_ids = Vec::new();

        // Sort services by dependencies
        let ordered = self.sort_services(&compose);

        for service_name in ordered {
            if let Some(service) = compose.services.get(&service_name) {
                let image = service.image.as_deref().unwrap_or(&service_name);
                let container_name = format!("{}_{}", stack_name, service_name);
                let mut container = Container::new(&container_name, image);

                // Add ports
                for port in &service.ports {
                    if let Some((host, cont)) = port.split_once(':') {
                        if let (Ok(h), Ok(c)) = (host.parse::<u16>(), cont.parse::<u16>()) {
                            container.ports.push(PortMapping {
                                host_port: h,
                                container_port: c,
                                protocol: Protocol::Tcp,
                                host_ip: None,
                            });
                        }
                    }
                }

                // Add environment
                container.env = service.environment.clone();

                // Add volumes
                for vol in &service.volumes {
                    if let Some((src, tgt)) = vol.split_once(':') {
                        container.volumes.push(VolumeMount {
                            source: src.to_string(),
                            target: tgt.to_string(),
                            read_only: false,
                            mount_type: MountType::Bind,
                        });
                    }
                }

                let result = self.container_manager.run(container);
                if result.success() {
                    container_ids.push(result.stdout.clone());
                }
            }
        }

        self.stacks
            .insert(stack_name.to_string(), container_ids.clone());
        CommandResult::ok(&format!("Started {} containers", container_ids.len()))
    }

    /// Down a stack
    pub fn down(&mut self, stack_name: &str) -> CommandResult {
        if let Some(container_ids) = self.stacks.remove(stack_name) {
            for id in &container_ids {
                self.container_manager.remove(id, true);
            }
            CommandResult::ok(&format!("Stopped {} containers", container_ids.len()))
        } else {
            CommandResult::error(1, "Stack not found")
        }
    }

    /// Sort services by dependencies (simple topological sort)
    fn sort_services(&self, compose: &ComposeFile) -> Vec<String> {
        let mut sorted = Vec::new();
        let mut visited = std::collections::HashSet::new();

        for name in compose.services.keys() {
            visit_service_recursive(name, compose, &mut visited, &mut sorted);
        }

        sorted
    }

    /// Get stack status
    pub fn status(&self, stack_name: &str) -> Option<Vec<&Container>> {
        self.stacks.get(stack_name).map(|ids| {
            ids.iter()
                .filter_map(|id| self.container_manager.get_container(id))
                .collect()
        })
    }
}

/// Helper for topological sort - visits a service and its dependencies
fn visit_service_recursive(
    name: &str,
    compose: &ComposeFile,
    visited: &mut std::collections::HashSet<String>,
    sorted: &mut Vec<String>,
) {
    if visited.contains(name) {
        return;
    }

    visited.insert(name.to_string());

    if let Some(service) = compose.services.get(name) {
        for dep in &service.depends_on {
            visit_service_recursive(dep, compose, visited, sorted);
        }
    }

    sorted.push(name.to_string());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_runtime_type_command() {
        assert_eq!(RuntimeType::Docker.command(), "docker");
        assert_eq!(RuntimeType::Podman.command(), "podman");
    }

    #[test]
    fn test_container_status_from_str() {
        assert_eq!(
            ContainerStatus::from_str("running"),
            ContainerStatus::Running
        );
        assert_eq!(
            ContainerStatus::from_str("stopped"),
            ContainerStatus::Stopped
        );
        assert_eq!(ContainerStatus::from_str("exited"), ContainerStatus::Exited);
        assert_eq!(
            ContainerStatus::from_str("unknown"),
            ContainerStatus::Unknown
        );
    }

    #[test]
    fn test_container_new() {
        let container = Container::new("myapp", "nginx:latest");
        assert_eq!(container.name, "myapp");
        assert_eq!(container.image, "nginx:latest");
        assert_eq!(container.status, ContainerStatus::Creating);
    }

    #[test]
    fn test_container_with_port() {
        let container = Container::new("web", "nginx").with_port(8080, 80);
        assert_eq!(container.ports.len(), 1);
        assert_eq!(container.ports[0].host_port, 8080);
        assert_eq!(container.ports[0].container_port, 80);
    }

    #[test]
    fn test_container_with_env() {
        let container = Container::new("app", "myapp").with_env("NODE_ENV", "production");
        assert_eq!(
            container.env.get("NODE_ENV"),
            Some(&"production".to_string())
        );
    }

    #[test]
    fn test_container_with_volume() {
        let container =
            Container::new("db", "postgres").with_volume("/data", "/var/lib/postgresql/data");
        assert_eq!(container.volumes.len(), 1);
        assert_eq!(container.volumes[0].source, "/data");
    }

    #[test]
    fn test_port_mapping_to_docker_arg() {
        let pm = PortMapping {
            host_port: 8080,
            container_port: 80,
            protocol: Protocol::Tcp,
            host_ip: None,
        };
        assert_eq!(pm.to_docker_arg(), "8080:80");

        let pm_udp = PortMapping {
            host_port: 53,
            container_port: 53,
            protocol: Protocol::Udp,
            host_ip: Some("127.0.0.1".to_string()),
        };
        assert_eq!(pm_udp.to_docker_arg(), "127.0.0.1:53:53/udp");
    }

    #[test]
    fn test_volume_mount_to_docker_arg() {
        let vm = VolumeMount {
            source: "/host/path".to_string(),
            target: "/container/path".to_string(),
            read_only: false,
            mount_type: MountType::Bind,
        };
        assert_eq!(vm.to_docker_arg(), "/host/path:/container/path");

        let vm_ro = VolumeMount {
            source: "myvolume".to_string(),
            target: "/data".to_string(),
            read_only: true,
            mount_type: MountType::Volume,
        };
        assert_eq!(vm_ro.to_docker_arg(), "myvolume:/data:ro");
    }

    #[test]
    fn test_image_parse_reference() {
        assert_eq!(
            Image::parse_reference("nginx"),
            ("nginx".to_string(), "latest".to_string())
        );
        assert_eq!(
            Image::parse_reference("nginx:1.21"),
            ("nginx".to_string(), "1.21".to_string())
        );
        assert_eq!(
            Image::parse_reference("registry.io/image:tag"),
            ("registry.io/image".to_string(), "tag".to_string())
        );
    }

    #[test]
    fn test_volume_new() {
        let vol = Volume::new("mydata");
        assert_eq!(vol.name, "mydata");
        assert_eq!(vol.driver, "local");
    }

    #[test]
    fn test_network_new() {
        let net = Network::new("mynet").with_driver("bridge");
        assert_eq!(net.name, "mynet");
        assert_eq!(net.driver, "bridge");
    }

    #[test]
    fn test_build_context() {
        let ctx = BuildContext::new(".")
            .with_tag("myimage:latest")
            .with_dockerfile("Dockerfile.prod")
            .with_arg("VERSION", "1.0")
            .no_cache();

        assert_eq!(ctx.context_path, ".");
        assert_eq!(ctx.tag, Some("myimage:latest".to_string()));
        assert!(ctx.no_cache);
    }

    #[test]
    fn test_exec_config() {
        let cfg = ExecConfig::new(&["ls", "-la"])
            .in_dir("/app")
            .with_env("PATH", "/usr/bin")
            .as_user("root")
            .with_tty();

        assert_eq!(cfg.command, vec!["ls", "-la"]);
        assert_eq!(cfg.workdir, Some("/app".to_string()));
        assert!(cfg.tty);
    }

    #[test]
    fn test_command_result() {
        let ok = CommandResult::ok("success");
        assert!(ok.success());
        assert_eq!(ok.stdout, "success");

        let err = CommandResult::error(1, "failed");
        assert!(!err.success());
        assert_eq!(err.exit_code, 1);
    }

    #[test]
    fn test_container_manager_new() {
        let mgr = ContainerManager::new(RuntimeType::Docker);
        assert_eq!(mgr.runtime, RuntimeType::Docker);
    }

    #[test]
    fn test_container_manager_build_run_command() {
        let mgr = ContainerManager::new(RuntimeType::Docker);
        let container = Container::new("myapp", "nginx")
            .with_port(8080, 80)
            .with_env("ENV", "prod");

        let cmd = mgr.build_run_command(&container);
        assert!(cmd.contains(&"docker".to_string()));
        assert!(cmd.contains(&"run".to_string()));
        assert!(cmd.contains(&"-p".to_string()));
        assert!(cmd.contains(&"8080:80".to_string()));
        assert!(cmd.contains(&"-e".to_string()));
        assert!(cmd.contains(&"ENV=prod".to_string()));
    }

    #[test]
    fn test_container_manager_run() {
        let mut mgr = ContainerManager::default();
        let container = Container::new("test", "alpine");

        let result = mgr.run(container);
        assert!(result.success());
        assert!(!result.stdout.is_empty());
    }

    #[test]
    fn test_container_manager_stop_start() {
        let mut mgr = ContainerManager::default();
        let container = Container::new("test", "alpine");
        let run_result = mgr.run(container);
        let container_id = run_result.stdout.clone();

        let stop_result = mgr.stop(&container_id);
        assert!(stop_result.success());

        let container = mgr.get_container(&container_id).unwrap();
        assert_eq!(container.status, ContainerStatus::Stopped);

        let start_result = mgr.start(&container_id);
        assert!(start_result.success());

        let container = mgr.get_container(&container_id).unwrap();
        assert_eq!(container.status, ContainerStatus::Running);
    }

    #[test]
    fn test_container_manager_remove() {
        let mut mgr = ContainerManager::default();
        let container = Container::new("test", "alpine");
        let run_result = mgr.run(container);
        let container_id = run_result.stdout;

        let result = mgr.remove(&container_id, false);
        assert!(result.success());
        assert!(mgr.get_container(&container_id).is_none());
    }

    #[test]
    fn test_container_manager_exec() {
        let mut mgr = ContainerManager::default();
        let container = Container::new("test", "alpine");
        let run_result = mgr.run(container);
        let container_id = run_result.stdout;

        let config = ExecConfig::new(&["ls", "-la"]);
        let result = mgr.exec(&container_id, config);
        assert!(result.success());
    }

    #[test]
    fn test_container_manager_build() {
        let mut mgr = ContainerManager::default();
        let ctx = BuildContext::new(".").with_tag("myimage:latest");

        let result = mgr.build(ctx);
        assert!(result.success());
        assert!(!mgr.images.is_empty());
    }

    #[test]
    fn test_container_manager_pull() {
        let mut mgr = ContainerManager::default();

        let result = mgr.pull("nginx:latest");
        assert!(result.success());

        let images = mgr.list_images();
        assert!(!images.is_empty());
    }

    #[test]
    fn test_container_manager_list() {
        let mut mgr = ContainerManager::default();

        mgr.run(Container::new("c1", "alpine"));
        mgr.run(Container::new("c2", "alpine"));

        let running = mgr.list_containers(false);
        assert_eq!(running.len(), 2);

        let all = mgr.list_containers(true);
        assert_eq!(all.len(), 2);
    }

    #[test]
    fn test_compose_service() {
        let service = ComposeService::new("web")
            .with_image("nginx")
            .with_port("8080:80")
            .with_env("ENV", "production")
            .depends("db");

        assert_eq!(service.name, "web");
        assert_eq!(service.image, Some("nginx".to_string()));
        assert!(service.depends_on.contains(&"db".to_string()));
    }

    #[test]
    fn test_compose_file_new() {
        let mut compose = ComposeFile::new();
        compose.add_service(ComposeService::new("web").with_image("nginx"));
        compose.add_volume("data");
        compose.add_network("frontend");

        assert_eq!(compose.services.len(), 1);
        assert_eq!(compose.volumes.len(), 1);
        assert_eq!(compose.networks.len(), 1);
    }

    #[test]
    fn test_compose_file_to_yaml() {
        let mut compose = ComposeFile::new();
        compose.add_service(
            ComposeService::new("web")
                .with_image("nginx:latest")
                .with_port("8080:80"),
        );

        let yaml = compose.to_yaml();
        assert!(yaml.contains("version:"));
        assert!(yaml.contains("services:"));
        assert!(yaml.contains("web:"));
        assert!(yaml.contains("nginx:latest"));
    }

    #[test]
    fn test_compose_file_parse_yaml() {
        let yaml = r#"
version: "3.8"
services:
  web:
    image: nginx
    restart: always
  db:
    image: postgres
"#;

        let compose = ComposeFile::parse_yaml(yaml).unwrap();
        assert_eq!(compose.version, "3.8");
        assert_eq!(compose.services.len(), 2);
        assert!(compose.services.contains_key("web"));
        assert!(compose.services.contains_key("db"));
    }

    #[test]
    fn test_compose_manager_new() {
        let mgr = ComposeManager::new();
        assert!(mgr.compose_files.is_empty());
    }

    #[test]
    fn test_compose_manager_load() {
        let mut mgr = ComposeManager::new();
        let yaml = r#"
version: "3.8"
services:
  app:
    image: myapp:latest
"#;

        assert!(mgr.load("mystack", yaml));
        assert!(mgr.compose_files.contains_key("mystack"));
    }

    #[test]
    fn test_compose_manager_up_down() {
        let mut mgr = ComposeManager::new();
        let yaml = r#"
version: "3.8"
services:
  web:
    image: nginx
  api:
    image: myapi
"#;

        mgr.load("stack", yaml);

        let up_result = mgr.up("stack");
        assert!(up_result.success());
        assert!(mgr.stacks.contains_key("stack"));

        let down_result = mgr.down("stack");
        assert!(down_result.success());
        assert!(!mgr.stacks.contains_key("stack"));
    }

    #[test]
    fn test_container_history() {
        let mut mgr = ContainerManager::default();
        mgr.run(Container::new("test", "alpine"));
        mgr.pull("nginx");

        let history = mgr.get_history();
        assert!(!history.is_empty());

        mgr.clear_history();
        assert!(mgr.get_history().is_empty());
    }

    #[test]
    fn test_container_is_running() {
        let mut container = Container::new("test", "alpine");
        assert!(!container.is_running());

        container.status = ContainerStatus::Running;
        assert!(container.is_running());
    }

    #[test]
    fn test_image_reference() {
        let image = Image {
            id: "sha256:abc".to_string(),
            repository: "nginx".to_string(),
            tag: "latest".to_string(),
            size: 1000,
            created_at: 0,
            labels: HashMap::new(),
        };

        assert_eq!(image.reference(), "nginx:latest");
    }

    #[test]
    fn test_build_exec_command() {
        let mgr = ContainerManager::new(RuntimeType::Docker);
        let config = ExecConfig::new(&["sh", "-c", "echo hello"])
            .in_dir("/app")
            .with_tty()
            .interactive();

        let cmd = mgr.build_exec_command("container123", &config);
        assert!(cmd.contains(&"-i".to_string()));
        assert!(cmd.contains(&"-t".to_string()));
        assert!(cmd.contains(&"-w".to_string()));
        assert!(cmd.contains(&"/app".to_string()));
        assert!(cmd.contains(&"container123".to_string()));
    }

    #[test]
    fn test_build_build_command() {
        let mgr = ContainerManager::new(RuntimeType::Docker);
        let ctx = BuildContext::new(".")
            .with_tag("myimage:1.0")
            .with_dockerfile("Dockerfile.prod")
            .with_arg("VERSION", "1.0")
            .with_target("production")
            .no_cache()
            .pull_latest();

        let cmd = mgr.build_build_command(&ctx);
        assert!(cmd.contains(&"-t".to_string()));
        assert!(cmd.contains(&"myimage:1.0".to_string()));
        assert!(cmd.contains(&"-f".to_string()));
        assert!(cmd.contains(&"--target".to_string()));
        assert!(cmd.contains(&"--no-cache".to_string()));
        assert!(cmd.contains(&"--pull".to_string()));
    }

    #[test]
    fn test_volume_with_driver() {
        let vol = Volume::new("data").with_driver("nfs");
        assert_eq!(vol.driver, "nfs");
    }

    #[test]
    fn test_network_with_subnet() {
        let net = Network::new("internal").with_subnet("10.0.0.0/24");
        assert_eq!(net.subnet, Some("10.0.0.0/24".to_string()));
    }

    #[test]
    fn test_compose_service_chain() {
        let service = ComposeService::new("api")
            .with_image("myapi:latest")
            .with_build("./api")
            .with_port("3000:3000")
            .with_volume("./data:/app/data")
            .with_restart("unless-stopped")
            .depends("db")
            .depends("redis");

        assert_eq!(service.depends_on.len(), 2);
        assert!(service.build.is_some());
        assert!(service.image.is_some());
    }
}
