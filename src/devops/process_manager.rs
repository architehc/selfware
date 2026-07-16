//! Process Manager - Background Process Lifecycle Management
//!
//! Enables long-running processes like dev servers, file watchers, and database
//! connections to persist across agent steps. Key features:
//!
//! - Health checks with regex patterns (e.g., "Compiled successfully")
//! - Log tailing for LLM context (last N lines)
//! - Auto-restart on crash with backoff
//! - Port management and conflict detection
//! - Graceful shutdown with cleanup
//!
//! This is essential for web/mobile development workflows where `npm run dev`
//! or `cargo watch` need to stay alive while the agent makes changes.

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::{Mutex, RwLock};
use tracing::{debug, info, warn};

/// Maximum number of log lines to keep per process
const MAX_LOG_LINES: usize = 500;

/// Maximum length of a single log line in bytes (10 KB)
const MAX_LOG_LINE_LEN: usize = 10_240;

/// Default health check timeout in seconds
const HEALTH_CHECK_TIMEOUT_SECS: u64 = 60;
/// How long a reserved port is kept before being released automatically.
const PORT_RESERVATION_TTL: Duration = Duration::from_secs(30);

/// Process status
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ProcessStatus {
    Starting,
    Running,
    HealthCheckFailed,
    Stopped,
    Crashed { exit_code: Option<i32> },
    Restarting { attempt: u32 },
}

/// Configuration for starting a managed process
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProcessConfig {
    /// Unique identifier for this process
    pub id: String,
    /// Command to execute (e.g., "npm", "cargo")
    pub command: String,
    /// Command arguments
    pub args: Vec<String>,
    /// Working directory
    pub cwd: Option<PathBuf>,
    /// Environment variables to set
    pub env: HashMap<String, String>,
    /// Regex pattern that indicates the process is healthy/ready
    /// e.g., "Compiled successfully|Ready on http"
    pub health_check_pattern: Option<String>,
    /// Timeout for health check in seconds
    pub health_check_timeout_secs: Option<u64>,
    /// Port the process is expected to listen on
    pub expected_port: Option<u16>,
    /// Whether to auto-restart on crash
    pub auto_restart: bool,
    /// Maximum restart attempts (0 = unlimited)
    pub max_restart_attempts: u32,
}

struct PortReservation {
    listener: tokio::net::TcpListener,
    reserved_at: Instant,
}

/// A managed background process
#[derive(Debug)]
pub struct ManagedProcess {
    pub config: ProcessConfig,
    pub status: ProcessStatus,
    pub pid: Option<u32>,
    pub started_at: Option<DateTime<Utc>>,
    pub log_buffer: VecDeque<LogLine>,
    pub health_matched: bool,
    pub restart_count: u32,
    child_handle: Option<Arc<RwLock<Option<Child>>>>,
}

/// A line from process output
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogLine {
    pub timestamp: DateTime<Utc>,
    pub stream: LogStream,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum LogStream {
    Stdout,
    Stderr,
}

/// Summary of a managed process for serialization
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessSummary {
    pub id: String,
    pub command: String,
    pub args: Vec<String>,
    pub status: ProcessStatus,
    pub pid: Option<u32>,
    pub started_at: Option<DateTime<Utc>>,
    pub uptime_secs: Option<i64>,
    pub health_matched: bool,
    pub restart_count: u32,
    pub expected_port: Option<u16>,
    pub recent_logs: Vec<LogLine>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProcessInventory {
    pub total: usize,
    pub running: usize,
    pub starting: usize,
    pub restarting: usize,
    pub inactive: usize,
    pub reserved_ports: Vec<u16>,
    pub processes: Vec<ProcessSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProcessReconcileReport {
    pub scanned: usize,
    pub orphaned_entries: usize,
    pub exited_processes: usize,
    pub handles_cleared: usize,
    pub removed_inactive: usize,
    pub reserved_ports: usize,
}

impl ManagedProcess {
    fn new(config: ProcessConfig) -> Self {
        Self {
            config,
            status: ProcessStatus::Stopped,
            pid: None,
            started_at: None,
            log_buffer: VecDeque::with_capacity(MAX_LOG_LINES),
            health_matched: false,
            restart_count: 0,
            child_handle: None,
        }
    }

    fn add_log(&mut self, stream: LogStream, content: String) {
        if self.log_buffer.len() >= MAX_LOG_LINES {
            self.log_buffer.pop_front();
        }
        let content = if content.len() > MAX_LOG_LINE_LEN {
            let mut truncated: String = content.chars().take(MAX_LOG_LINE_LEN).collect();
            truncated.push_str("...[truncated]");
            truncated
        } else {
            content
        };
        self.log_buffer.push_back(LogLine {
            timestamp: Utc::now(),
            stream,
            content,
        });
    }

    fn to_summary(&self, log_lines: usize) -> ProcessSummary {
        let recent_logs: Vec<LogLine> = self
            .log_buffer
            .iter()
            .rev()
            .take(log_lines)
            .cloned()
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();

        let uptime_secs = self
            .started_at
            .map(|started| (Utc::now() - started).num_seconds());

        ProcessSummary {
            id: self.config.id.clone(),
            command: self.config.command.clone(),
            args: self.config.args.clone(),
            status: self.status.clone(),
            pid: self.pid,
            started_at: self.started_at,
            uptime_secs,
            health_matched: self.health_matched,
            restart_count: self.restart_count,
            expected_port: self.config.expected_port,
            recent_logs,
        }
    }
}

/// Manager for background processes
pub struct ProcessManager {
    processes: Arc<RwLock<HashMap<String, ManagedProcess>>>,
    port_reservations: Arc<Mutex<HashMap<u16, PortReservation>>>,
}

impl ProcessManager {
    pub fn new() -> Self {
        Self {
            processes: Arc::new(RwLock::new(HashMap::new())),
            port_reservations: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    async fn cleanup_stale_port_reservations(&self) {
        let mut reservations = self.port_reservations.lock().await;
        reservations.retain(|port, reservation| {
            let keep = reservation.reserved_at.elapsed() <= PORT_RESERVATION_TTL;
            if !keep {
                warn!(
                    "Dropping stale reserved port {} after {:?}",
                    port, PORT_RESERVATION_TTL
                );
            }
            keep
        });
    }

    pub async fn has_reserved_port(&self, port: u16) -> bool {
        self.cleanup_stale_port_reservations().await;
        let reservations = self.port_reservations.lock().await;
        reservations.contains_key(&port)
    }

    pub async fn reserve_port(&self, port: u16) -> Result<u16> {
        // Hold the reservation lock for the whole check+bind+insert sequence so
        // two tasks cannot reserve the same port concurrently.
        let mut reservations = self.port_reservations.lock().await;
        reservations.retain(|port, reservation| {
            let keep = reservation.reserved_at.elapsed() <= PORT_RESERVATION_TTL;
            if !keep {
                warn!(
                    "Dropping stale reserved port {} after {:?}",
                    port, PORT_RESERVATION_TTL
                );
            }
            keep
        });

        if reservations.contains_key(&port) {
            anyhow::bail!("Port {} is already reserved by selfware", port);
        }

        let listener = tokio::net::TcpListener::bind(("127.0.0.1", port))
            .await
            .with_context(|| format!("Port {} is already in use", port))?;

        reservations.insert(
            port,
            PortReservation {
                listener,
                reserved_at: Instant::now(),
            },
        );
        Ok(port)
    }

    pub async fn reserve_available_port(&self, start: u16, end: u16) -> Result<u16> {
        // Hold the reservation lock while scanning and binding so another task
        // cannot slip in and reserve a port we are about to claim.
        let mut reservations = self.port_reservations.lock().await;
        reservations.retain(|port, reservation| {
            let keep = reservation.reserved_at.elapsed() <= PORT_RESERVATION_TTL;
            if !keep {
                warn!(
                    "Dropping stale reserved port {} after {:?}",
                    port, PORT_RESERVATION_TTL
                );
            }
            keep
        });

        for port in start..=end {
            if reservations.contains_key(&port) {
                continue;
            }

            if let Ok(listener) = tokio::net::TcpListener::bind(("127.0.0.1", port)).await {
                reservations.insert(
                    port,
                    PortReservation {
                        listener,
                        reserved_at: Instant::now(),
                    },
                );
                return Ok(port);
            }
        }

        anyhow::bail!("No available ports found in range {}-{}", start, end)
    }

    async fn acquire_startup_port_listener(&self, port: u16) -> Result<tokio::net::TcpListener> {
        if let Some(listener) = self.take_reserved_port(port).await {
            return Ok(listener);
        }

        self.reserve_port(port).await?;
        self.take_reserved_port(port)
            .await
            .context("Reserved port disappeared before process start")
    }

    async fn take_reserved_port(&self, port: u16) -> Option<tokio::net::TcpListener> {
        self.cleanup_stale_port_reservations().await;
        let mut reservations = self.port_reservations.lock().await;
        reservations
            .remove(&port)
            .map(|reservation| reservation.listener)
    }

    pub async fn release_reserved_port(&self, port: u16) -> bool {
        let mut reservations = self.port_reservations.lock().await;
        reservations.remove(&port).is_some()
    }

    pub async fn clear_port_reservations(&self) -> usize {
        let mut reservations = self.port_reservations.lock().await;
        let count = reservations.len();
        reservations.clear();
        count
    }

    pub async fn inventory(&self, log_lines: usize) -> ProcessInventory {
        self.cleanup_stale_port_reservations().await;
        let processes = self.processes.read().await;
        let mut inventory = ProcessInventory {
            total: processes.len(),
            processes: processes
                .values()
                .map(|p| p.to_summary(log_lines))
                .collect(),
            ..Default::default()
        };
        inventory.processes.sort_by(|a, b| a.id.cmp(&b.id));

        for process in &inventory.processes {
            match process.status {
                ProcessStatus::Running => inventory.running += 1,
                ProcessStatus::Starting => inventory.starting += 1,
                ProcessStatus::Restarting { .. } => inventory.restarting += 1,
                ProcessStatus::Stopped
                | ProcessStatus::HealthCheckFailed
                | ProcessStatus::Crashed { .. } => inventory.inactive += 1,
            }
        }

        let reservations = self.port_reservations.lock().await;
        inventory.reserved_ports = reservations.keys().copied().collect();
        inventory.reserved_ports.sort_unstable();
        inventory
    }

    pub fn try_inventory(&self, log_lines: usize) -> Option<ProcessInventory> {
        let processes = self.processes.try_read().ok()?;
        let reservations = self.port_reservations.try_lock().ok()?;

        let mut inventory = ProcessInventory {
            total: processes.len(),
            processes: processes
                .values()
                .map(|p| p.to_summary(log_lines))
                .collect(),
            ..Default::default()
        };
        inventory.processes.sort_by(|a, b| a.id.cmp(&b.id));

        for process in &inventory.processes {
            match process.status {
                ProcessStatus::Running => inventory.running += 1,
                ProcessStatus::Starting => inventory.starting += 1,
                ProcessStatus::Restarting { .. } => inventory.restarting += 1,
                ProcessStatus::Stopped
                | ProcessStatus::HealthCheckFailed
                | ProcessStatus::Crashed { .. } => inventory.inactive += 1,
            }
        }

        inventory.reserved_ports = reservations.keys().copied().collect();
        inventory.reserved_ports.sort_unstable();
        Some(inventory)
    }

    pub async fn reconcile(&self, prune_inactive: bool) -> ProcessReconcileReport {
        self.cleanup_stale_port_reservations().await;

        let ids: Vec<String> = {
            let processes = self.processes.read().await;
            processes.keys().cloned().collect()
        };

        let mut report = ProcessReconcileReport {
            scanned: ids.len(),
            reserved_ports: self.port_reservations.lock().await.len(),
            ..Default::default()
        };

        for id in ids {
            let child_handle = {
                let processes = self.processes.read().await;
                processes
                    .get(&id)
                    .and_then(|proc| proc.child_handle.clone())
            };

            let mut observed_exit_code = None;
            let mut cleared_handle = false;
            let mut missing_running_handle = false;

            if let Some(handle) = child_handle {
                let mut child_guard = handle.write().await;
                if let Some(child) = child_guard.as_mut() {
                    if let Some(status) = child.try_wait().ok().flatten() {
                        observed_exit_code = Some(status.code());
                        *child_guard = None;
                        cleared_handle = true;
                    }
                } else {
                    missing_running_handle = true;
                }
            } else {
                missing_running_handle = true;
            }

            let mut processes = self.processes.write().await;
            if let Some(proc) = processes.get_mut(&id) {
                if let Some(exit_code) = observed_exit_code {
                    report.exited_processes += 1;
                    if cleared_handle {
                        report.handles_cleared += 1;
                    }
                    proc.child_handle = None;
                    proc.pid = None;
                    if !matches!(
                        proc.status,
                        ProcessStatus::Stopped | ProcessStatus::HealthCheckFailed
                    ) {
                        proc.status = ProcessStatus::Crashed { exit_code };
                    }
                } else if missing_running_handle
                    && matches!(
                        proc.status,
                        ProcessStatus::Running
                            | ProcessStatus::Starting
                            | ProcessStatus::Restarting { .. }
                    )
                {
                    report.orphaned_entries += 1;
                    proc.child_handle = None;
                    proc.pid = None;
                    proc.status = ProcessStatus::Crashed { exit_code: None };
                }
            }
        }

        if prune_inactive {
            let mut processes = self.processes.write().await;
            let before = processes.len();
            processes.retain(|_, proc| {
                matches!(
                    proc.status,
                    ProcessStatus::Running
                        | ProcessStatus::Starting
                        | ProcessStatus::Restarting { .. }
                )
            });
            report.removed_inactive = before.saturating_sub(processes.len());
        }

        report
    }

    /// Start a new managed process
    pub async fn start(&self, config: ProcessConfig) -> Result<ProcessSummary> {
        let id = config.id.clone();

        // Check if process with this ID already exists and is running
        {
            let processes = self.processes.read().await;
            if let Some(existing) = processes.get(&id) {
                if matches!(
                    existing.status,
                    ProcessStatus::Running
                        | ProcessStatus::Starting
                        | ProcessStatus::Restarting { .. }
                ) {
                    if existing.config == config {
                        info!("Reusing existing managed process '{}'", id);
                        return Ok(existing.to_summary(50));
                    }
                    anyhow::bail!(
                        "Process '{}' is already running with a different configuration",
                        id
                    );
                }
            }
        }

        let health_pattern = config
            .health_check_pattern
            .as_ref()
            .map(|p| Regex::new(p))
            .transpose()
            .context("Invalid health check regex pattern")?;

        let health_timeout = config
            .health_check_timeout_secs
            .unwrap_or(HEALTH_CHECK_TIMEOUT_SECS);

        let reserved_port_listener = match config.expected_port {
            Some(port) => Some(self.acquire_startup_port_listener(port).await?),
            None => None,
        };

        // Build the command
        let mut cmd = Command::new(&config.command);
        cmd.args(&config.args);

        if let Some(ref cwd) = config.cwd {
            cmd.current_dir(cwd);
        }

        // Clear the inherited environment to prevent secret leakage (e.g.
        // SELFWARE_API_KEY) into child processes, then set a minimal base. This
        // matches spawn_child_process (the restart path already does this); the
        // initial spawn must be consistent.
        cmd.env_clear();
        if let Ok(path) = std::env::var("PATH") {
            cmd.env("PATH", path);
        }
        if let Ok(home) = std::env::var("HOME") {
            cmd.env("HOME", home);
        }
        if let Ok(lang) = std::env::var("LANG") {
            cmd.env("LANG", lang);
        }
        for (key, value) in &config.env {
            cmd.env(key, value);
        }

        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());
        cmd.kill_on_drop(true); // Ensure child is killed if handle is dropped to prevent zombies

        info!(
            "Starting process '{}': {} {:?}",
            id, config.command, config.args
        );

        // Release the reservation at the last possible moment before spawning the child
        // so the process can bind to the port immediately.
        drop(reserved_port_listener);

        let child = cmd.spawn().with_context(|| {
            format!(
                "Failed to spawn process: {} {:?}",
                config.command, config.args
            )
        })?;

        let pid = child.id();
        let child_handle = Arc::new(RwLock::new(Some(child)));

        // Create the managed process entry
        let mut managed = ManagedProcess::new(config.clone());
        managed.status = ProcessStatus::Starting;
        managed.pid = pid;
        managed.started_at = Some(Utc::now());
        managed.child_handle = Some(child_handle.clone());

        // Store the process
        {
            let mut processes = self.processes.write().await;
            processes.insert(id.clone(), managed);
        }

        // Spawn log collection tasks
        let processes_clone = self.processes.clone();
        let id_clone = id.clone();

        // Get stdout/stderr from child
        let mut child_guard = child_handle.write().await;
        if let Some(ref mut child) = *child_guard {
            if let Some(stdout) = child.stdout.take() {
                let processes = processes_clone.clone();
                let id = id_clone.clone();
                let health_pattern_clone = health_pattern.clone();

                tokio::spawn(async move {
                    collect_output(
                        processes,
                        id,
                        stdout,
                        LogStream::Stdout,
                        health_pattern_clone,
                    )
                    .await;
                });
            }

            if let Some(stderr) = child.stderr.take() {
                let processes = processes_clone.clone();
                let id = id_clone.clone();

                tokio::spawn(async move {
                    collect_output(processes, id, stderr, LogStream::Stderr, None).await;
                });
            }
        }
        drop(child_guard);

        // Spawn process monitor task
        let processes_monitor = self.processes.clone();
        let id_monitor = id.clone();
        let child_handle_monitor = child_handle.clone();
        let auto_restart = config.auto_restart;
        let max_restarts = config.max_restart_attempts;

        tokio::spawn(async move {
            monitor_process(
                processes_monitor,
                id_monitor,
                child_handle_monitor,
                auto_restart,
                max_restarts,
            )
            .await;
        });

        // Wait for health check if pattern specified
        if health_pattern.is_some() {
            let start = std::time::Instant::now();
            let timeout = std::time::Duration::from_secs(health_timeout);

            loop {
                if start.elapsed() > timeout {
                    warn!("Health check timeout for process '{}'", id);

                    let (exit_code, timed_out_while_running) = {
                        let mut child_guard = child_handle.write().await;
                        if let Some(mut child) = child_guard.take() {
                            if let Some(status) = child.try_wait().ok().flatten() {
                                (status.code(), false)
                            } else {
                                warn!(
                                    "Process '{}' failed health check and will be terminated",
                                    id
                                );
                                let _ = child.kill().await;
                                let exit_code = child.wait().await.ok().and_then(|s| s.code());
                                (exit_code, true)
                            }
                        } else {
                            (None, false)
                        }
                    };

                    let mut processes = self.processes.write().await;
                    if let Some(proc) = processes.get_mut(&id) {
                        proc.child_handle = None;
                        proc.pid = None;
                        if timed_out_while_running {
                            proc.status = ProcessStatus::HealthCheckFailed;
                        } else if let Some(code) = exit_code {
                            proc.status = ProcessStatus::Crashed {
                                exit_code: Some(code),
                            };
                        } else {
                            proc.status = ProcessStatus::HealthCheckFailed;
                        }
                    }
                    break;
                }

                {
                    let processes = self.processes.read().await;
                    if let Some(proc) = processes.get(&id) {
                        if proc.health_matched {
                            info!("Process '{}' passed health check", id);
                            break;
                        }
                        if matches!(
                            proc.status,
                            ProcessStatus::Crashed { .. } | ProcessStatus::Stopped
                        ) {
                            break;
                        }
                    }
                }

                tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
            }
        } else {
            // No health check: give the process a brief window to settle, but
            // POLL for an early exit throughout it instead of taking a single
            // snapshot at the end. A single 500 ms snapshot was flaky: under a
            // saturated CPU the child could exit slightly later, and the
            // concurrent `monitor_process` task races us to reap it (whoever
            // calls `try_wait` first gets the code, the other gets `None`), so
            // an immediately-crashing process was sometimes mis-marked Running.
            // Break as soon as either we or the monitor observe the exit.
            let deadline = tokio::time::Instant::now() + tokio::time::Duration::from_millis(800);
            let mut exit_code = None;
            loop {
                {
                    let mut child_guard = child_handle.write().await;
                    if let Some(ref mut child) = *child_guard {
                        if let Some(code) = child.try_wait().ok().flatten().and_then(|s| s.code()) {
                            exit_code = Some(code);
                        }
                    }
                }
                // The monitor task may have reaped + recorded the exit first.
                let monitor_saw_exit = {
                    let processes = self.processes.read().await;
                    processes.get(&id).is_some_and(|p| {
                        matches!(
                            p.status,
                            ProcessStatus::Crashed { .. } | ProcessStatus::Stopped
                        )
                    })
                };
                if exit_code.is_some()
                    || monitor_saw_exit
                    || tokio::time::Instant::now() >= deadline
                {
                    break;
                }
                tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
            }

            let mut processes = self.processes.write().await;
            if let Some(proc) = processes.get_mut(&id) {
                if let Some(code) = exit_code {
                    proc.status = ProcessStatus::Crashed {
                        exit_code: Some(code),
                    };
                } else if matches!(proc.status, ProcessStatus::Starting) {
                    // Neither we nor the monitor saw an exit within the window.
                    proc.status = ProcessStatus::Running;
                    proc.health_matched = true;
                }
                // else: the monitor already set Crashed/Stopped — leave it.
            }
        }

        // Return summary -- check for failure states and return errors
        let processes = self.processes.read().await;
        let proc = processes
            .get(&id)
            .ok_or_else(|| anyhow::anyhow!("Process disappeared after start"))?;

        let summary = proc.to_summary(50);
        match &summary.status {
            ProcessStatus::HealthCheckFailed => {
                let recent_output: Vec<&str> = summary
                    .recent_logs
                    .iter()
                    .rev()
                    .take(5)
                    .map(|l| l.content.as_str())
                    .collect();
                anyhow::bail!(
                    "Process '{}' started but health check timed out after {}s. \
                     Recent output: {:?}",
                    id,
                    health_timeout,
                    recent_output
                );
            }
            ProcessStatus::Crashed { exit_code } => {
                let recent_output: Vec<&str> = summary
                    .recent_logs
                    .iter()
                    .rev()
                    .take(5)
                    .map(|l| l.content.as_str())
                    .collect();
                anyhow::bail!(
                    "Process '{}' exited immediately with code {}. Recent output: {:?}",
                    id,
                    exit_code
                        .map(|c| c.to_string())
                        .unwrap_or_else(|| "unknown".to_string()),
                    recent_output
                );
            }
            ProcessStatus::Stopped => {
                anyhow::bail!("Process '{}' was stopped before it could become ready", id);
            }
            _ => Ok(summary),
        }
    }

    /// Stop a managed process
    pub async fn stop(&self, id: &str, force: bool) -> Result<ProcessSummary> {
        let mut processes = self.processes.write().await;
        let proc = processes
            .get_mut(id)
            .ok_or_else(|| anyhow::anyhow!("Process '{}' not found", id))?;

        if matches!(
            proc.status,
            ProcessStatus::Stopped | ProcessStatus::Crashed { .. }
        ) {
            return Ok(proc.to_summary(20));
        }

        info!("Stopping process '{}' (force={})", id, force);

        if let Some(ref child_handle) = proc.child_handle {
            let mut child_guard = child_handle.write().await;
            if let Some(ref mut child) = *child_guard {
                if force {
                    let _ = child.kill().await;
                    let _ = child.wait().await; // reap zombie
                } else {
                    // Try graceful shutdown first
                    #[cfg(unix)]
                    {
                        use nix::sys::signal::{kill, Signal};
                        use nix::unistd::Pid;
                        if let Some(pid) = proc.pid {
                            if let Ok(raw_pid) = i32::try_from(pid) {
                                let _ = kill(Pid::from_raw(raw_pid), Signal::SIGTERM);
                            } else {
                                warn!(
                                    "Skipping SIGTERM for pid {}: does not fit into platform pid_t",
                                    pid
                                );
                                let _ = child.kill().await;
                                let _ = child.wait().await;
                            }
                        }
                    }
                    #[cfg(not(unix))]
                    {
                        let _ = child.kill().await;
                        let _ = child.wait().await;
                    }

                    // Wait up to 3 seconds for graceful exit, then force kill
                    match tokio::time::timeout(std::time::Duration::from_secs(3), child.wait())
                        .await
                    {
                        Ok(_) => {} // Process exited
                        Err(_) => {
                            // Timeout — force kill and reap
                            warn!(
                                "Process '{}' did not exit after SIGTERM, sending SIGKILL",
                                id
                            );
                            let _ = child.kill().await;
                            let _ = child.wait().await;
                        }
                    }
                }
            }
        }

        proc.status = ProcessStatus::Stopped;
        proc.pid = None;

        Ok(proc.to_summary(20))
    }

    /// Stop all running managed processes gracefully.
    ///
    /// Returns the number of processes that were actually stopped.
    pub async fn stop_all(&self) -> usize {
        let ids: Vec<String> = {
            let processes = self.processes.read().await;
            processes
                .iter()
                .filter(|(_, p)| {
                    matches!(
                        p.status,
                        ProcessStatus::Running
                            | ProcessStatus::Starting
                            | ProcessStatus::Restarting { .. }
                    )
                })
                .map(|(id, _)| id.clone())
                .collect()
        };

        let mut stopped = 0;
        for id in &ids {
            match self.stop(id, false).await {
                Ok(_) => {
                    info!("Stopped managed process '{}'", id);
                    stopped += 1;
                }
                Err(e) => {
                    warn!("Failed to stop process '{}': {}", id, e);
                }
            }
        }
        stopped
    }

    /// List all managed processes
    pub async fn list(&self) -> Vec<ProcessSummary> {
        let processes = self.processes.read().await;
        processes.values().map(|p| p.to_summary(10)).collect()
    }

    /// Get logs for a specific process
    pub async fn logs(&self, id: &str, lines: usize) -> Result<Vec<LogLine>> {
        let processes = self.processes.read().await;
        let proc = processes
            .get(id)
            .ok_or_else(|| anyhow::anyhow!("Process '{}' not found", id))?;

        Ok(proc
            .log_buffer
            .iter()
            .rev()
            .take(lines)
            .cloned()
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect())
    }

    /// Get a process summary
    pub async fn get(&self, id: &str) -> Result<ProcessSummary> {
        let processes = self.processes.read().await;
        let proc = processes
            .get(id)
            .ok_or_else(|| anyhow::anyhow!("Process '{}' not found", id))?;

        Ok(proc.to_summary(20))
    }

    /// Remove a stopped process from management
    pub async fn remove(&self, id: &str) -> Result<()> {
        let mut processes = self.processes.write().await;
        let proc = processes
            .get(id)
            .ok_or_else(|| anyhow::anyhow!("Process '{}' not found", id))?;

        if matches!(
            proc.status,
            ProcessStatus::Running | ProcessStatus::Starting
        ) {
            anyhow::bail!("Cannot remove running process '{}'. Stop it first.", id);
        }

        processes.remove(id);
        Ok(())
    }

    /// Restart a process
    pub async fn restart(&self, id: &str) -> Result<ProcessSummary> {
        let config = {
            let processes = self.processes.read().await;
            let proc = processes
                .get(id)
                .ok_or_else(|| anyhow::anyhow!("Process '{}' not found", id))?;
            proc.config.clone()
        };

        // Stop if running
        let _ = self.stop(id, false).await;
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

        // Remove old entry
        {
            let mut processes = self.processes.write().await;
            processes.remove(id);
        }

        // Start fresh
        self.start(config).await
    }
}

impl Default for ProcessManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Spawn a child process from config (used by start and restart)
async fn spawn_child_process(
    config: &ProcessConfig,
) -> Result<(Option<u32>, Arc<RwLock<Option<Child>>>)> {
    let mut cmd = Command::new(&config.command);
    cmd.args(&config.args);

    if let Some(ref cwd) = config.cwd {
        cmd.current_dir(cwd);
    }

    // Clear inherited environment to prevent secret leakage, then set a minimal base
    cmd.env_clear();
    if let Ok(path) = std::env::var("PATH") {
        cmd.env("PATH", path);
    }
    if let Ok(home) = std::env::var("HOME") {
        cmd.env("HOME", home);
    }
    if let Ok(lang) = std::env::var("LANG") {
        cmd.env("LANG", lang);
    }
    for (key, value) in &config.env {
        cmd.env(key, value);
    }

    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());
    cmd.kill_on_drop(true); // Ensure child is killed if handle is dropped to prevent zombies

    let child = cmd.spawn().with_context(|| {
        format!(
            "Failed to spawn process: {} {:?}",
            config.command, config.args
        )
    })?;

    let pid = child.id();
    let child_handle = Arc::new(RwLock::new(Some(child)));

    Ok((pid, child_handle))
}

/// Collect output from a process stream
async fn collect_output<R: tokio::io::AsyncRead + Unpin>(
    processes: Arc<RwLock<HashMap<String, ManagedProcess>>>,
    id: String,
    reader: R,
    stream: LogStream,
    health_pattern: Option<Regex>,
) {
    let mut lines = BufReader::new(reader).lines();

    while let Ok(Some(line)) = lines.next_line().await {
        debug!("[{}] {:?}: {}", id, stream, line);

        // Check health pattern
        if let Some(ref pattern) = health_pattern {
            if pattern.is_match(&line) {
                let mut procs = processes.write().await;
                if let Some(proc) = procs.get_mut(&id) {
                    if !proc.health_matched {
                        proc.health_matched = true;
                        proc.status = ProcessStatus::Running;
                        info!("Process '{}' health check passed: {}", id, line);
                    }
                }
            }
        }

        // Store log line
        let mut procs = processes.write().await;
        if let Some(proc) = procs.get_mut(&id) {
            proc.add_log(stream.clone(), line);
        }
    }
}

/// Monitor a process for exit
async fn monitor_process(
    processes: Arc<RwLock<HashMap<String, ManagedProcess>>>,
    id: String,
    child_handle: Arc<RwLock<Option<Child>>>,
    auto_restart: bool,
    max_restarts: u32,
) {
    loop {
        // Poll for process exit using try_wait() with short lock holds.
        // This avoids holding the child_handle write lock during a blocking
        // wait, which would prevent stop() from acquiring the lock to kill
        // the child process.
        let exit_status = loop {
            let try_result = {
                let mut child_guard = child_handle.write().await;
                if let Some(ref mut child) = *child_guard {
                    child.try_wait().ok().flatten()
                } else {
                    // Child was taken/killed by stop(), treat as stopped
                    break None;
                }
            };
            // Lock is dropped here

            if let Some(status) = try_result {
                break Some(status);
            }

            // Check if process was marked as stopped by stop()
            {
                let procs = processes.read().await;
                if let Some(proc) = procs.get(&id) {
                    if matches!(proc.status, ProcessStatus::Stopped) {
                        break None;
                    }
                }
            }

            // Sleep briefly before polling again
            tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
        };

        let Some(status) = exit_status else {
            // Child handle was empty or process was stopped externally.
            // Exit the monitor loop.
            break;
        };

        let exit_code = status.code();
        warn!("Process '{}' exited with code: {:?}", id, exit_code);

        let mut procs = processes.write().await;
        if let Some(proc) = procs.get_mut(&id) {
            let should_restart = auto_restart
                && (max_restarts == 0 || proc.restart_count < max_restarts)
                && !matches!(proc.status, ProcessStatus::Stopped);

            if should_restart {
                proc.restart_count += 1;
                let restart_attempt = proc.restart_count;
                proc.status = ProcessStatus::Restarting {
                    attempt: restart_attempt,
                };
                info!(
                    "Auto-restarting process '{}' (attempt {})",
                    id, restart_attempt
                );

                // Clone config for restart
                let config = proc.config.clone();
                let health_pattern = config
                    .health_check_pattern
                    .as_ref()
                    .and_then(|p| Regex::new(p).ok());

                // Backoff delay
                let delay = std::cmp::min(restart_attempt * 2, 30);
                drop(procs);
                tokio::time::sleep(tokio::time::Duration::from_secs(delay as u64)).await;

                // Actually restart the process
                match spawn_child_process(&config).await {
                    Ok((pid, new_child_handle)) => {
                        // Update process state
                        {
                            let mut procs = processes.write().await;
                            if let Some(proc) = procs.get_mut(&id) {
                                proc.pid = pid;
                                proc.started_at = Some(Utc::now());
                                proc.status = ProcessStatus::Starting;
                                proc.health_matched = false;
                                proc.child_handle = Some(new_child_handle.clone());
                            }
                        }

                        // Setup output collection for the new process
                        {
                            let mut child_guard = new_child_handle.write().await;
                            if let Some(ref mut child) = *child_guard {
                                if let Some(stdout) = child.stdout.take() {
                                    let procs = processes.clone();
                                    let proc_id = id.clone();
                                    let hp = health_pattern.clone();
                                    tokio::spawn(async move {
                                        collect_output(
                                            procs,
                                            proc_id,
                                            stdout,
                                            LogStream::Stdout,
                                            hp,
                                        )
                                        .await;
                                    });
                                }
                                if let Some(stderr) = child.stderr.take() {
                                    let procs = processes.clone();
                                    let proc_id = id.clone();
                                    tokio::spawn(async move {
                                        collect_output(
                                            procs,
                                            proc_id,
                                            stderr,
                                            LogStream::Stderr,
                                            None,
                                        )
                                        .await;
                                    });
                                }
                            }
                        }

                        // Update child_handle for continued monitoring
                        // Move the child from new_child_handle to the original child_handle
                        let new_child = new_child_handle.write().await.take();
                        *child_handle.write().await = new_child;

                        // Mark as running after brief startup
                        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
                        let mut procs = processes.write().await;
                        if let Some(proc) = procs.get_mut(&id) {
                            if matches!(proc.status, ProcessStatus::Starting)
                                && health_pattern.is_none()
                            {
                                proc.status = ProcessStatus::Running;
                                proc.health_matched = true;
                            }
                        }

                        info!(
                            "Process '{}' restarted successfully (attempt {})",
                            id, restart_attempt
                        );
                        // Continue monitoring loop
                        continue;
                    }
                    Err(e) => {
                        warn!("Failed to restart process '{}': {}", id, e);
                        let mut procs = processes.write().await;
                        if let Some(proc) = procs.get_mut(&id) {
                            proc.status = ProcessStatus::Crashed { exit_code };
                        }
                    }
                }
            } else if !matches!(proc.status, ProcessStatus::Stopped) {
                proc.status = ProcessStatus::Crashed { exit_code };
            }
        }
        break;
    }
}

/// Check if a port is available.
///
/// NOTE: This has a TOCTOU race -- the port may be taken between the check and
/// actual use. Prefer `bind_available_port` when you need to guarantee the port
/// stays reserved.
pub async fn is_port_available(port: u16) -> bool {
    tokio::net::TcpListener::bind(("127.0.0.1", port))
        .await
        .is_ok()
}

/// Find an available port in a range.
///
/// NOTE: This has a TOCTOU race -- the port may be taken between the check and
/// actual use. Prefer `bind_available_port` when you need to guarantee the port
/// stays reserved.
pub async fn find_available_port(start: u16, end: u16) -> Option<u16> {
    for port in start..=end {
        if is_port_available(port).await {
            return Some(port);
        }
    }
    None
}

/// Bind to an available port and return the listener with the assigned port.
///
/// Uses port 0 to let the OS assign a free port, eliminating the TOCTOU race
/// condition present in `is_port_available`/`find_available_port`. The caller
/// should hold the returned `TcpListener` until the child process has bound to
/// the port (or pass the port via env/arg and drop the listener right before
/// the child binds).
pub async fn bind_available_port() -> Option<(tokio::net::TcpListener, u16)> {
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0u16))
        .await
        .ok()?;
    let port = listener.local_addr().ok()?.port();
    Some((listener, port))
}

/// Check what's listening on a port (Unix only)
#[cfg(unix)]
pub async fn port_info(port: u16) -> Option<String> {
    let output = tokio::process::Command::new("lsof")
        .args(["-i", &format!(":{}", port), "-P", "-n"])
        .output()
        .await
        .ok()?;

    if output.status.success() {
        Some(String::from_utf8_lossy(&output.stdout).to_string())
    } else {
        None
    }
}

#[cfg(not(unix))]
pub async fn port_info(_port: u16) -> Option<String> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_process_config_creation() {
        let config = ProcessConfig {
            id: "test".to_string(),
            command: "echo".to_string(),
            args: vec!["hello".to_string()],
            cwd: None,
            env: HashMap::new(),
            health_check_pattern: None,
            health_check_timeout_secs: None,
            expected_port: None,
            auto_restart: false,
            max_restart_attempts: 0,
        };

        assert_eq!(config.id, "test");
        assert_eq!(config.command, "echo");
    }

    #[test]
    fn test_process_status_serde() {
        let status = ProcessStatus::Running;
        let json = serde_json::to_string(&status).unwrap();
        assert_eq!(json, "\"running\"");

        let crashed = ProcessStatus::Crashed { exit_code: Some(1) };
        let json = serde_json::to_string(&crashed).unwrap();
        assert!(json.contains("crashed"));
    }

    #[test]
    fn test_log_line_creation() {
        let log = LogLine {
            timestamp: Utc::now(),
            stream: LogStream::Stdout,
            content: "test output".to_string(),
        };

        assert_eq!(log.stream, LogStream::Stdout);
        assert_eq!(log.content, "test output");
    }

    #[test]
    fn test_managed_process_log_buffer() {
        let config = ProcessConfig {
            id: "test".to_string(),
            command: "echo".to_string(),
            args: vec![],
            cwd: None,
            env: HashMap::new(),
            health_check_pattern: None,
            health_check_timeout_secs: None,
            expected_port: None,
            auto_restart: false,
            max_restart_attempts: 0,
        };

        let mut proc = ManagedProcess::new(config);

        // Add some logs
        proc.add_log(LogStream::Stdout, "line 1".to_string());
        proc.add_log(LogStream::Stderr, "error 1".to_string());
        proc.add_log(LogStream::Stdout, "line 2".to_string());

        assert_eq!(proc.log_buffer.len(), 3);
    }

    #[test]
    fn test_managed_process_to_summary() {
        let config = ProcessConfig {
            id: "test-server".to_string(),
            command: "npm".to_string(),
            args: vec!["run".to_string(), "dev".to_string()],
            cwd: None,
            env: HashMap::new(),
            health_check_pattern: Some("Ready".to_string()),
            health_check_timeout_secs: Some(30),
            expected_port: Some(3000),
            auto_restart: true,
            max_restart_attempts: 3,
        };

        let mut proc = ManagedProcess::new(config);
        proc.status = ProcessStatus::Running;
        proc.pid = Some(12345);
        proc.started_at = Some(Utc::now());
        proc.health_matched = true;

        let summary = proc.to_summary(10);

        assert_eq!(summary.id, "test-server");
        assert_eq!(summary.command, "npm");
        assert_eq!(summary.status, ProcessStatus::Running);
        assert_eq!(summary.pid, Some(12345));
        assert!(summary.health_matched);
        assert_eq!(summary.expected_port, Some(3000));
    }

    #[tokio::test]
    async fn test_port_availability() {
        // Port 0 lets the OS assign a free port
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();

        // Port should be in use
        assert!(!is_port_available(port).await);

        // Drop the listener
        drop(listener);

        // Port should now be available (might need a small delay on some systems)
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        assert!(is_port_available(port).await);
    }

    #[tokio::test(start_paused = true)]
    async fn test_find_available_port() {
        // Find a port in a high range that's likely free
        let port = find_available_port(50000, 50100).await;
        assert!(port.is_some());
    }

    #[tokio::test(start_paused = true)]
    async fn test_process_manager_new() {
        let manager = ProcessManager::new();
        let list = manager.list().await;
        assert!(list.is_empty());
    }

    #[tokio::test(start_paused = true)]
    async fn test_process_manager_start_simple() {
        let manager = ProcessManager::new();

        // Use sleep which stays alive, unlike echo which exits immediately
        let config = ProcessConfig {
            id: "echo-test".to_string(),
            command: "sleep".to_string(),
            args: vec!["60".to_string()],
            cwd: None,
            env: HashMap::new(),
            health_check_pattern: None,
            health_check_timeout_secs: None,
            expected_port: None,
            auto_restart: false,
            max_restart_attempts: 0,
        };

        let result = manager.start(config).await;
        assert!(result.is_ok());

        let summary = result.unwrap();
        assert_eq!(summary.id, "echo-test");
        assert!(summary.pid.is_some());

        // Cleanup
        let _ = manager.stop("echo-test", true).await;
    }

    #[tokio::test(start_paused = true)]
    async fn test_process_manager_list() {
        let manager = ProcessManager::new();

        let config = ProcessConfig {
            id: "list-test".to_string(),
            command: "sleep".to_string(),
            args: vec!["0.1".to_string()],
            cwd: None,
            env: HashMap::new(),
            health_check_pattern: None,
            health_check_timeout_secs: None,
            expected_port: None,
            auto_restart: false,
            max_restart_attempts: 0,
        };

        let _ = manager.start(config).await;

        let list = manager.list().await;
        assert!(!list.is_empty());
        assert!(list.iter().any(|p| p.id == "list-test"));
    }

    #[tokio::test(start_paused = true)]
    async fn test_process_manager_stop() {
        let manager = ProcessManager::new();

        let config = ProcessConfig {
            id: "stop-test".to_string(),
            command: "sleep".to_string(),
            args: vec!["60".to_string()],
            cwd: None,
            env: HashMap::new(),
            health_check_pattern: None,
            health_check_timeout_secs: None,
            expected_port: None,
            auto_restart: false,
            max_restart_attempts: 0,
        };

        let _ = manager.start(config).await;

        let result = manager.stop("stop-test", false).await;
        assert!(result.is_ok());

        let summary = result.unwrap();
        assert_eq!(summary.status, ProcessStatus::Stopped);
    }

    #[tokio::test(start_paused = true)]
    async fn test_process_manager_get() {
        let manager = ProcessManager::new();

        let config = ProcessConfig {
            id: "get-test".to_string(),
            command: "echo".to_string(),
            args: vec!["test".to_string()],
            cwd: None,
            env: HashMap::new(),
            health_check_pattern: None,
            health_check_timeout_secs: None,
            expected_port: None,
            auto_restart: false,
            max_restart_attempts: 0,
        };

        let _ = manager.start(config).await;

        let result = manager.get("get-test").await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().id, "get-test");
    }

    #[tokio::test(start_paused = true)]
    async fn test_process_manager_get_not_found() {
        let manager = ProcessManager::new();
        let result = manager.get("nonexistent").await;
        assert!(result.is_err());
    }

    #[tokio::test(start_paused = true)]
    async fn test_process_manager_logs() {
        let manager = ProcessManager::new();

        let config = ProcessConfig {
            id: "logs-test".to_string(),
            command: "echo".to_string(),
            args: vec!["hello world".to_string()],
            cwd: None,
            env: HashMap::new(),
            health_check_pattern: None,
            health_check_timeout_secs: None,
            expected_port: None,
            auto_restart: false,
            max_restart_attempts: 0,
        };

        let _ = manager.start(config).await;

        // Give it time to capture output
        tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

        let logs = manager.logs("logs-test", 10).await;
        assert!(logs.is_ok());
    }

    #[tokio::test(start_paused = true)]
    async fn test_process_manager_duplicate_start_reuses_matching_config() {
        let manager = ProcessManager::new();

        let config = ProcessConfig {
            id: "dup-test".to_string(),
            command: "sleep".to_string(),
            args: vec!["60".to_string()],
            cwd: None,
            env: HashMap::new(),
            health_check_pattern: None,
            health_check_timeout_secs: None,
            expected_port: None,
            auto_restart: false,
            max_restart_attempts: 0,
        };

        let first = manager.start(config.clone()).await.unwrap();

        // Try to start again with same ID
        let second = manager.start(config).await.unwrap();
        assert_eq!(first.id, second.id);
        assert_eq!(first.pid, second.pid);

        // Cleanup
        let _ = manager.stop("dup-test", true).await;
    }

    #[tokio::test(start_paused = true)]
    async fn test_process_manager_duplicate_start_different_config_errors() {
        let manager = ProcessManager::new();

        let config = ProcessConfig {
            id: "dup-config-test".to_string(),
            command: "sleep".to_string(),
            args: vec!["60".to_string()],
            cwd: None,
            env: HashMap::new(),
            health_check_pattern: None,
            health_check_timeout_secs: None,
            expected_port: None,
            auto_restart: false,
            max_restart_attempts: 0,
        };

        let _ = manager.start(config.clone()).await.unwrap();

        let mut changed = config;
        changed.args = vec!["30".to_string()];

        let result = manager.start(changed).await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("different configuration"));

        let _ = manager.stop("dup-config-test", true).await;
    }

    #[tokio::test]
    async fn test_port_reservation_round_trip() {
        let manager = ProcessManager::new();
        let port = manager.reserve_available_port(55000, 55100).await.unwrap();
        assert!(manager.has_reserved_port(port).await);
        assert!(!is_port_available(port).await);

        let released = manager.release_reserved_port(port).await;
        assert!(released);

        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
        assert!(is_port_available(port).await);
    }

    #[tokio::test]
    async fn test_start_consumes_matching_port_reservation() {
        let manager = ProcessManager::new();
        let port = manager.reserve_available_port(55101, 55200).await.unwrap();
        assert!(manager.has_reserved_port(port).await);

        let config = ProcessConfig {
            id: "reserved-port-start-test".to_string(),
            command: "sleep".to_string(),
            args: vec!["60".to_string()],
            cwd: None,
            env: HashMap::new(),
            health_check_pattern: None,
            health_check_timeout_secs: None,
            expected_port: Some(port),
            auto_restart: false,
            max_restart_attempts: 0,
        };

        let summary = manager.start(config).await.unwrap();
        assert_eq!(summary.expected_port, Some(port));
        assert!(!manager.has_reserved_port(port).await);

        let _ = manager.stop("reserved-port-start-test", true).await;
    }

    #[tokio::test]
    async fn test_acquire_startup_port_listener_auto_reserves_unreserved_port() {
        // Retry with fresh ports to avoid TOCTOU races: between dropping the
        // probe listener and calling acquire_startup_port_listener, another
        // process (or parallel test) may grab the port.
        let mut last_err = None;
        for _ in 0..5 {
            let manager = ProcessManager::new();
            let (probe_listener, port) = bind_available_port().await.unwrap();
            drop(probe_listener);

            match manager.acquire_startup_port_listener(port).await {
                Ok(startup_listener) => {
                    assert!(!manager.has_reserved_port(port).await);
                    assert!(!is_port_available(port).await);

                    drop(startup_listener);
                    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
                    assert!(is_port_available(port).await);
                    return; // success
                }
                Err(e) => {
                    last_err = Some(e);
                    continue;
                }
            }
        }
        panic!(
            "Failed after 5 attempts to acquire an unreserved port: {}",
            last_err.unwrap()
        );
    }

    #[tokio::test]
    async fn test_invalid_health_check_regex_preserves_existing_port_reservation() {
        let manager = ProcessManager::new();
        let port = manager.reserve_available_port(55201, 55300).await.unwrap();
        assert!(manager.has_reserved_port(port).await);

        let config = ProcessConfig {
            id: "invalid-regex-reservation-test".to_string(),
            command: "sleep".to_string(),
            args: vec!["1".to_string()],
            cwd: None,
            env: HashMap::new(),
            health_check_pattern: Some("(".to_string()),
            health_check_timeout_secs: None,
            expected_port: Some(port),
            auto_restart: false,
            max_restart_attempts: 0,
        };

        let error = manager.start(config).await.unwrap_err();
        assert!(error
            .to_string()
            .contains("Invalid health check regex pattern"));
        assert!(manager.has_reserved_port(port).await);

        assert!(manager.release_reserved_port(port).await);
    }

    #[tokio::test]
    async fn test_process_manager_with_env() {
        let manager = ProcessManager::new();

        let mut env = HashMap::new();
        env.insert("TEST_VAR".to_string(), "test_value".to_string());

        let config = ProcessConfig {
            id: "env-test".to_string(),
            command: "sh".to_string(),
            args: vec!["-c".to_string(), "echo $TEST_VAR".to_string()],
            cwd: None,
            env,
            health_check_pattern: None,
            health_check_timeout_secs: None,
            expected_port: None,
            auto_restart: false,
            max_restart_attempts: 0,
        };

        // Process exits immediately after echo, so start returns Err
        let _ = manager.start(config).await;

        // Give it time to capture output
        tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

        // The process entry still exists and logs were captured before it exited
        let logs = manager.logs("env-test", 10).await.unwrap();
        assert!(logs.iter().any(|l| l.content.contains("test_value")));
    }

    #[tokio::test]
    async fn test_process_manager_remove() {
        let manager = ProcessManager::new();

        let config = ProcessConfig {
            id: "remove-test".to_string(),
            command: "echo".to_string(),
            args: vec!["test".to_string()],
            cwd: None,
            env: HashMap::new(),
            health_check_pattern: None,
            health_check_timeout_secs: None,
            expected_port: None,
            auto_restart: false,
            max_restart_attempts: 0,
        };

        let _ = manager.start(config).await;

        // Wait for it to finish
        tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

        let result = manager.remove("remove-test").await;
        assert!(result.is_ok());

        // Should not be in list anymore
        let list = manager.list().await;
        assert!(!list.iter().any(|p| p.id == "remove-test"));
    }

    #[test]
    fn test_log_stream_serde() {
        let stdout = LogStream::Stdout;
        let json = serde_json::to_string(&stdout).unwrap();
        assert_eq!(json, "\"stdout\"");

        let stderr = LogStream::Stderr;
        let json = serde_json::to_string(&stderr).unwrap();
        assert_eq!(json, "\"stderr\"");
    }

    #[test]
    fn test_process_status_variants() {
        let starting = ProcessStatus::Starting;
        assert!(matches!(starting, ProcessStatus::Starting));

        let restarting = ProcessStatus::Restarting { attempt: 2 };
        if let ProcessStatus::Restarting { attempt } = restarting {
            assert_eq!(attempt, 2);
        }

        let health_failed = ProcessStatus::HealthCheckFailed;
        assert!(matches!(health_failed, ProcessStatus::HealthCheckFailed));
    }

    #[test]
    fn test_process_summary_serde() {
        let summary = ProcessSummary {
            id: "test".to_string(),
            command: "node".to_string(),
            args: vec!["server.js".to_string()],
            status: ProcessStatus::Running,
            pid: Some(12345),
            started_at: Some(Utc::now()),
            uptime_secs: Some(60),
            health_matched: true,
            restart_count: 0,
            expected_port: Some(3000),
            recent_logs: vec![],
        };

        let json = serde_json::to_string(&summary).unwrap();
        assert!(json.contains("test"));
        assert!(json.contains("node"));
        assert!(json.contains("3000"));
    }

    #[test]
    fn test_process_status_clone() {
        let status = ProcessStatus::Running;
        let cloned = status.clone();
        assert_eq!(status, cloned);

        let crashed = ProcessStatus::Crashed { exit_code: Some(1) };
        let cloned = crashed.clone();
        assert_eq!(crashed, cloned);
    }

    #[test]
    fn test_process_status_debug() {
        let status = ProcessStatus::Starting;
        let debug_str = format!("{:?}", status);
        assert!(debug_str.contains("Starting"));

        let restarting = ProcessStatus::Restarting { attempt: 3 };
        let debug_str = format!("{:?}", restarting);
        assert!(debug_str.contains("Restarting"));
        assert!(debug_str.contains("3"));
    }

    #[test]
    fn test_process_status_all_variants() {
        let variants = [
            ProcessStatus::Starting,
            ProcessStatus::Running,
            ProcessStatus::HealthCheckFailed,
            ProcessStatus::Stopped,
            ProcessStatus::Crashed { exit_code: None },
            ProcessStatus::Crashed {
                exit_code: Some(127),
            },
            ProcessStatus::Restarting { attempt: 1 },
        ];
        for v in variants {
            let _ = serde_json::to_string(&v).unwrap();
        }
    }

    #[test]
    fn test_process_status_eq() {
        assert_eq!(ProcessStatus::Running, ProcessStatus::Running);
        assert_ne!(ProcessStatus::Running, ProcessStatus::Stopped);
        assert_ne!(
            ProcessStatus::Crashed { exit_code: Some(1) },
            ProcessStatus::Crashed { exit_code: Some(2) }
        );
    }

    #[test]
    fn test_process_config_clone() {
        let config = ProcessConfig {
            id: "test".to_string(),
            command: "echo".to_string(),
            args: vec!["hello".to_string()],
            cwd: Some(PathBuf::from("/tmp")),
            env: HashMap::new(),
            health_check_pattern: Some("Ready".to_string()),
            health_check_timeout_secs: Some(30),
            expected_port: Some(8080),
            auto_restart: true,
            max_restart_attempts: 5,
        };

        let cloned = config.clone();
        assert_eq!(config.id, cloned.id);
        assert_eq!(config.command, cloned.command);
        assert_eq!(config.expected_port, cloned.expected_port);
    }

    #[test]
    fn test_process_config_debug() {
        let config = ProcessConfig {
            id: "debug-test".to_string(),
            command: "node".to_string(),
            args: vec![],
            cwd: None,
            env: HashMap::new(),
            health_check_pattern: None,
            health_check_timeout_secs: None,
            expected_port: None,
            auto_restart: false,
            max_restart_attempts: 0,
        };

        let debug_str = format!("{:?}", config);
        assert!(debug_str.contains("ProcessConfig"));
        assert!(debug_str.contains("debug-test"));
    }

    #[test]
    fn test_process_config_with_all_options() {
        let mut env = HashMap::new();
        env.insert("PORT".to_string(), "3000".to_string());
        env.insert("NODE_ENV".to_string(), "development".to_string());

        let config = ProcessConfig {
            id: "full-config".to_string(),
            command: "npm".to_string(),
            args: vec!["run".to_string(), "start".to_string()],
            cwd: Some(PathBuf::from("/home/user/project")),
            env,
            health_check_pattern: Some(r"Listening on port \d+".to_string()),
            health_check_timeout_secs: Some(120),
            expected_port: Some(3000),
            auto_restart: true,
            max_restart_attempts: 10,
        };

        assert!(config.auto_restart);
        assert_eq!(config.max_restart_attempts, 10);
        assert_eq!(config.env.len(), 2);
    }

    #[test]
    fn test_log_line_clone() {
        let log = LogLine {
            timestamp: Utc::now(),
            stream: LogStream::Stdout,
            content: "test output".to_string(),
        };

        let cloned = log.clone();
        assert_eq!(log.stream, cloned.stream);
        assert_eq!(log.content, cloned.content);
    }

    #[test]
    fn test_log_line_debug() {
        let log = LogLine {
            timestamp: Utc::now(),
            stream: LogStream::Stderr,
            content: "error message".to_string(),
        };

        let debug_str = format!("{:?}", log);
        assert!(debug_str.contains("LogLine"));
        assert!(debug_str.contains("Stderr"));
    }

    #[test]
    fn test_log_line_serde_roundtrip() {
        let log = LogLine {
            timestamp: Utc::now(),
            stream: LogStream::Stdout,
            content: "test line".to_string(),
        };

        let json = serde_json::to_string(&log).unwrap();
        let parsed: LogLine = serde_json::from_str(&json).unwrap();

        assert_eq!(log.stream, parsed.stream);
        assert_eq!(log.content, parsed.content);
    }

    #[test]
    fn test_log_stream_clone() {
        let stream = LogStream::Stdout;
        let cloned = stream.clone();
        assert_eq!(stream, cloned);
    }

    #[test]
    fn test_log_stream_debug() {
        let stdout = LogStream::Stdout;
        assert!(format!("{:?}", stdout).contains("Stdout"));

        let stderr = LogStream::Stderr;
        assert!(format!("{:?}", stderr).contains("Stderr"));
    }

    #[test]
    fn test_log_stream_eq() {
        assert_eq!(LogStream::Stdout, LogStream::Stdout);
        assert_ne!(LogStream::Stdout, LogStream::Stderr);
    }

    #[test]
    fn test_process_summary_clone() {
        let summary = ProcessSummary {
            id: "clone-test".to_string(),
            command: "cargo".to_string(),
            args: vec!["run".to_string()],
            status: ProcessStatus::Running,
            pid: Some(999),
            started_at: Some(Utc::now()),
            uptime_secs: Some(100),
            health_matched: true,
            restart_count: 2,
            expected_port: Some(8000),
            recent_logs: vec![],
        };

        let cloned = summary.clone();
        assert_eq!(summary.id, cloned.id);
        assert_eq!(summary.restart_count, cloned.restart_count);
    }

    #[test]
    fn test_process_summary_debug() {
        let summary = ProcessSummary {
            id: "debug-test".to_string(),
            command: "python".to_string(),
            args: vec!["app.py".to_string()],
            status: ProcessStatus::Stopped,
            pid: None,
            started_at: None,
            uptime_secs: None,
            health_matched: false,
            restart_count: 0,
            expected_port: None,
            recent_logs: vec![],
        };

        let debug_str = format!("{:?}", summary);
        assert!(debug_str.contains("ProcessSummary"));
    }

    #[test]
    fn test_process_summary_deserialize() {
        let json = r#"{
            "id": "test",
            "command": "echo",
            "args": [],
            "status": "running",
            "pid": 12345,
            "started_at": null,
            "uptime_secs": null,
            "health_matched": true,
            "restart_count": 0,
            "expected_port": null,
            "recent_logs": []
        }"#;

        let summary: ProcessSummary = serde_json::from_str(json).unwrap();
        assert_eq!(summary.id, "test");
        assert_eq!(summary.status, ProcessStatus::Running);
    }

    #[test]
    fn test_managed_process_debug() {
        let config = ProcessConfig {
            id: "debug".to_string(),
            command: "ls".to_string(),
            args: vec![],
            cwd: None,
            env: HashMap::new(),
            health_check_pattern: None,
            health_check_timeout_secs: None,
            expected_port: None,
            auto_restart: false,
            max_restart_attempts: 0,
        };

        let proc = ManagedProcess::new(config);
        let debug_str = format!("{:?}", proc);
        assert!(debug_str.contains("ManagedProcess"));
    }

    #[test]
    fn test_managed_process_log_buffer_overflow() {
        let config = ProcessConfig {
            id: "overflow".to_string(),
            command: "test".to_string(),
            args: vec![],
            cwd: None,
            env: HashMap::new(),
            health_check_pattern: None,
            health_check_timeout_secs: None,
            expected_port: None,
            auto_restart: false,
            max_restart_attempts: 0,
        };

        let mut proc = ManagedProcess::new(config);

        // Add more than MAX_LOG_LINES
        for i in 0..600 {
            proc.add_log(LogStream::Stdout, format!("line {}", i));
        }

        // Should not exceed MAX_LOG_LINES
        assert!(proc.log_buffer.len() <= MAX_LOG_LINES);
    }

    #[test]
    fn test_process_summary_with_logs() {
        let config = ProcessConfig {
            id: "logs".to_string(),
            command: "echo".to_string(),
            args: vec![],
            cwd: None,
            env: HashMap::new(),
            health_check_pattern: None,
            health_check_timeout_secs: None,
            expected_port: None,
            auto_restart: false,
            max_restart_attempts: 0,
        };

        let mut proc = ManagedProcess::new(config);
        proc.add_log(LogStream::Stdout, "line 1".to_string());
        proc.add_log(LogStream::Stdout, "line 2".to_string());
        proc.add_log(LogStream::Stdout, "line 3".to_string());

        let summary = proc.to_summary(2);
        assert_eq!(summary.recent_logs.len(), 2);
        // Should be last 2 logs
        assert_eq!(summary.recent_logs[1].content, "line 3");
    }

    #[tokio::test(start_paused = true)]
    async fn test_process_manager_stop_nonexistent() {
        let manager = ProcessManager::new();
        let result = manager.stop("nonexistent", false).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }

    #[tokio::test(start_paused = true)]
    async fn test_process_manager_logs_nonexistent() {
        let manager = ProcessManager::new();
        let result = manager.logs("nonexistent", 10).await;
        assert!(result.is_err());
    }

    #[tokio::test(start_paused = true)]
    async fn test_process_manager_remove_nonexistent() {
        let manager = ProcessManager::new();
        let result = manager.remove("nonexistent").await;
        assert!(result.is_err());
    }

    #[test]
    fn test_process_config_serde_roundtrip() {
        let mut env = HashMap::new();
        env.insert("KEY".to_string(), "value".to_string());

        let config = ProcessConfig {
            id: "serde-test".to_string(),
            command: "cargo".to_string(),
            args: vec!["build".to_string(), "--release".to_string()],
            cwd: Some(PathBuf::from("/home/user/project")),
            env,
            health_check_pattern: Some("Finished".to_string()),
            health_check_timeout_secs: Some(60),
            expected_port: Some(8080),
            auto_restart: true,
            max_restart_attempts: 3,
        };

        let json = serde_json::to_string(&config).unwrap();
        let parsed: ProcessConfig = serde_json::from_str(&json).unwrap();

        assert_eq!(config.id, parsed.id);
        assert_eq!(config.command, parsed.command);
        assert_eq!(config.args, parsed.args);
        assert_eq!(config.expected_port, parsed.expected_port);
        assert_eq!(config.auto_restart, parsed.auto_restart);
    }

    #[test]
    fn test_process_config_minimal_serde() {
        let config = ProcessConfig {
            id: "minimal".to_string(),
            command: "ls".to_string(),
            args: vec![],
            cwd: None,
            env: HashMap::new(),
            health_check_pattern: None,
            health_check_timeout_secs: None,
            expected_port: None,
            auto_restart: false,
            max_restart_attempts: 0,
        };

        let json = serde_json::to_string(&config).unwrap();
        let parsed: ProcessConfig = serde_json::from_str(&json).unwrap();

        assert_eq!(config.id, parsed.id);
        assert!(parsed.cwd.is_none());
        assert!(parsed.health_check_pattern.is_none());
    }

    #[test]
    fn test_process_status_deserialize_all_variants() {
        let json_starting = r#""starting""#;
        let parsed: ProcessStatus = serde_json::from_str(json_starting).unwrap();
        assert!(matches!(parsed, ProcessStatus::Starting));

        let json_running = r#""running""#;
        let parsed: ProcessStatus = serde_json::from_str(json_running).unwrap();
        assert!(matches!(parsed, ProcessStatus::Running));

        let json_stopped = r#""stopped""#;
        let parsed: ProcessStatus = serde_json::from_str(json_stopped).unwrap();
        assert!(matches!(parsed, ProcessStatus::Stopped));

        let json_failed = r#""health_check_failed""#;
        let parsed: ProcessStatus = serde_json::from_str(json_failed).unwrap();
        assert!(matches!(parsed, ProcessStatus::HealthCheckFailed));
    }

    #[test]
    fn test_process_status_crashed_serde() {
        let crashed = ProcessStatus::Crashed { exit_code: Some(1) };
        let json = serde_json::to_string(&crashed).unwrap();
        let parsed: ProcessStatus = serde_json::from_str(&json).unwrap();

        if let ProcessStatus::Crashed { exit_code } = parsed {
            assert_eq!(exit_code, Some(1));
        } else {
            panic!("Expected Crashed variant");
        }
    }

    #[test]
    fn test_process_status_crashed_none_exit_code() {
        let crashed = ProcessStatus::Crashed { exit_code: None };
        let json = serde_json::to_string(&crashed).unwrap();
        let parsed: ProcessStatus = serde_json::from_str(&json).unwrap();

        if let ProcessStatus::Crashed { exit_code } = parsed {
            assert!(exit_code.is_none());
        } else {
            panic!("Expected Crashed variant");
        }
    }

    #[test]
    fn test_process_status_restarting_serde() {
        let restarting = ProcessStatus::Restarting { attempt: 5 };
        let json = serde_json::to_string(&restarting).unwrap();
        let parsed: ProcessStatus = serde_json::from_str(&json).unwrap();

        if let ProcessStatus::Restarting { attempt } = parsed {
            assert_eq!(attempt, 5);
        } else {
            panic!("Expected Restarting variant");
        }
    }

    #[tokio::test(start_paused = true)]
    async fn test_process_manager_default() {
        let manager = ProcessManager::default();
        let list = manager.list().await;
        assert!(list.is_empty());
    }

    #[tokio::test]
    async fn test_process_manager_reconcile_marks_orphaned_running_entry() {
        let manager = ProcessManager::new();

        let config = ProcessConfig {
            id: "orphaned".to_string(),
            command: "sleep".to_string(),
            args: vec!["60".to_string()],
            cwd: None,
            env: HashMap::new(),
            health_check_pattern: None,
            health_check_timeout_secs: None,
            expected_port: None,
            auto_restart: false,
            max_restart_attempts: 0,
        };

        let mut proc = ManagedProcess::new(config);
        proc.status = ProcessStatus::Running;
        proc.pid = Some(4242);

        manager
            .processes
            .write()
            .await
            .insert("orphaned".to_string(), proc);

        let report = manager.reconcile(false).await;
        assert_eq!(report.orphaned_entries, 1);

        let summary = manager.get("orphaned").await.unwrap();
        assert!(matches!(
            summary.status,
            ProcessStatus::Crashed { exit_code: None }
        ));
        assert!(summary.pid.is_none());
    }

    #[tokio::test]
    async fn test_process_manager_reconcile_prunes_inactive_entries() {
        let manager = ProcessManager::new();

        let mut proc = ManagedProcess::new(ProcessConfig {
            id: "stopped-entry".to_string(),
            command: "sleep".to_string(),
            args: vec!["60".to_string()],
            cwd: None,
            env: HashMap::new(),
            health_check_pattern: None,
            health_check_timeout_secs: None,
            expected_port: None,
            auto_restart: false,
            max_restart_attempts: 0,
        });
        proc.status = ProcessStatus::Stopped;

        manager
            .processes
            .write()
            .await
            .insert("stopped-entry".to_string(), proc);

        let report = manager.reconcile(true).await;
        assert_eq!(report.removed_inactive, 1);
        assert!(manager.list().await.is_empty());
    }

    #[tokio::test]
    async fn test_process_manager_inventory_includes_reserved_ports() {
        let manager = ProcessManager::new();
        let port = manager.reserve_available_port(56201, 56300).await.unwrap();

        let inventory = manager.inventory(5).await;
        assert_eq!(inventory.total, 0);
        assert_eq!(inventory.reserved_ports, vec![port]);

        assert!(manager.release_reserved_port(port).await);
    }

    /// Sentinel secret in the parent env must NOT leak into the child.
    #[tokio::test]
    async fn start_does_not_leak_parent_env_to_child() {
        // A sentinel secret present in the PARENT environment.
        std::env::set_var("SELFWARE_SENTINEL_SECRET", "leaked-value-123");

        let mgr = ProcessManager::new();
        let config = ProcessConfig {
            id: "env-leak-test".to_string(),
            command: "sh".to_string(),
            args: vec!["-c".to_string(), "env".to_string()],
            cwd: None,
            env: HashMap::new(),
            health_check_pattern: None,
            health_check_timeout_secs: None,
            expected_port: None,
            auto_restart: false,
            max_restart_attempts: 0,
        };

        // `env` exits immediately, so start() returns Err — that's fine; the
        // output is captured before the process exits.
        let _ = mgr.start(config).await;

        tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

        let logs = mgr.logs("env-leak-test", 200).await.unwrap();
        let output: String = logs
            .iter()
            .map(|l| l.content.as_str())
            .collect::<Vec<_>>()
            .join("\n");

        std::env::remove_var("SELFWARE_SENTINEL_SECRET");

        assert!(
            !output.contains("leaked-value-123"),
            "child environment leaked the parent secret: {output}"
        );
        // The minimal allowlist still passes PATH through.
        assert!(
            output.contains("PATH="),
            "PATH should be present in child env, got: {output}"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn test_process_manager_force_stop() {
        let manager = ProcessManager::new();

        let config = ProcessConfig {
            id: "force-stop-test".to_string(),
            command: "sleep".to_string(),
            args: vec!["60".to_string()],
            cwd: None,
            env: HashMap::new(),
            health_check_pattern: None,
            health_check_timeout_secs: None,
            expected_port: None,
            auto_restart: false,
            max_restart_attempts: 0,
        };

        let _ = manager.start(config).await;

        // Force stop
        let result = manager.stop("force-stop-test", true).await;
        assert!(result.is_ok());

        let summary = result.unwrap();
        assert_eq!(summary.status, ProcessStatus::Stopped);
    }

    #[tokio::test(start_paused = true)]
    async fn test_process_manager_stop_already_stopped() {
        let manager = ProcessManager::new();

        let config = ProcessConfig {
            id: "already-stopped".to_string(),
            command: "echo".to_string(),
            args: vec!["done".to_string()],
            cwd: None,
            env: HashMap::new(),
            health_check_pattern: None,
            health_check_timeout_secs: None,
            expected_port: None,
            auto_restart: false,
            max_restart_attempts: 0,
        };

        let _ = manager.start(config).await;
        tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

        // Stop should return ok even if already stopped
        let result = manager.stop("already-stopped", false).await;
        assert!(result.is_ok());
    }

    #[tokio::test(start_paused = true)]
    async fn test_process_manager_restart() {
        let manager = ProcessManager::new();

        let config = ProcessConfig {
            id: "restart-test".to_string(),
            command: "sleep".to_string(),
            args: vec!["60".to_string()],
            cwd: None,
            env: HashMap::new(),
            health_check_pattern: None,
            health_check_timeout_secs: None,
            expected_port: None,
            auto_restart: false,
            max_restart_attempts: 0,
        };

        let _ = manager.start(config).await;

        // Restart
        let result = manager.restart("restart-test").await;
        assert!(result.is_ok());

        let summary = result.unwrap();
        assert_eq!(summary.id, "restart-test");

        // Cleanup
        let _ = manager.stop("restart-test", true).await;
    }

    #[tokio::test(start_paused = true)]
    async fn test_process_manager_restart_nonexistent() {
        let manager = ProcessManager::new();
        let result = manager.restart("nonexistent").await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }

    #[tokio::test(start_paused = true)]
    async fn test_process_manager_remove_running() {
        let manager = ProcessManager::new();

        let config = ProcessConfig {
            id: "remove-running".to_string(),
            command: "sleep".to_string(),
            args: vec!["60".to_string()],
            cwd: None,
            env: HashMap::new(),
            health_check_pattern: None,
            health_check_timeout_secs: None,
            expected_port: None,
            auto_restart: false,
            max_restart_attempts: 0,
        };

        let _ = manager.start(config).await;

        // Try to remove while running
        let result = manager.remove("remove-running").await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Stop it first"));

        // Cleanup
        let _ = manager.stop("remove-running", true).await;
    }

    #[tokio::test]
    #[cfg(not(target_os = "windows"))]
    async fn test_process_manager_with_working_directory() {
        let manager = ProcessManager::new();

        let config = ProcessConfig {
            id: "cwd-test".to_string(),
            command: "pwd".to_string(),
            args: vec![],
            cwd: Some(PathBuf::from("/tmp")),
            env: HashMap::new(),
            health_check_pattern: None,
            health_check_timeout_secs: None,
            expected_port: None,
            auto_restart: false,
            max_restart_attempts: 0,
        };

        // pwd exits immediately, so start returns Err
        let _ = manager.start(config).await;

        tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

        // The process entry still exists and logs were captured
        let logs = manager.logs("cwd-test", 10).await.unwrap();
        assert!(logs.iter().any(|l| l.content.contains("/tmp")));
    }

    #[tokio::test(start_paused = true)]
    async fn test_process_manager_with_health_check() {
        let manager = ProcessManager::new();

        let config = ProcessConfig {
            id: "health-check-test".to_string(),
            command: "echo".to_string(),
            args: vec!["Server ready".to_string()],
            cwd: None,
            env: HashMap::new(),
            health_check_pattern: Some("ready".to_string()),
            health_check_timeout_secs: Some(5),
            expected_port: None,
            auto_restart: false,
            max_restart_attempts: 0,
        };

        let result = manager.start(config).await;
        // echo prints "Server ready" matching the health check, then exits.
        // Depending on task scheduling, health_matched may be set to true
        // before or after the monitor marks the process as Crashed.
        match result {
            Ok(summary) => {
                // Health check matched before crash was detected
                assert!(summary.health_matched);
                assert_eq!(summary.status, ProcessStatus::Running);
            }
            Err(e) => {
                // Process crashed before/after health check was evaluated
                let msg = e.to_string();
                assert!(
                    msg.contains("exited immediately") || msg.contains("health check"),
                    "Unexpected error: {}",
                    msg
                );
            }
        }
    }

    #[tokio::test(start_paused = true)]
    async fn test_process_summary_uptime() {
        let config = ProcessConfig {
            id: "uptime-test".to_string(),
            command: "sleep".to_string(),
            args: vec!["1".to_string()],
            cwd: None,
            env: HashMap::new(),
            health_check_pattern: None,
            health_check_timeout_secs: None,
            expected_port: None,
            auto_restart: false,
            max_restart_attempts: 0,
        };

        let mut proc = ManagedProcess::new(config);
        proc.started_at = Some(Utc::now() - chrono::Duration::seconds(10));
        proc.status = ProcessStatus::Running;

        let summary = proc.to_summary(10);
        assert!(summary.uptime_secs.is_some());
        assert!(summary.uptime_secs.unwrap() >= 10);
    }

    #[test]
    fn test_log_stream_serde_roundtrip() {
        let stdout = LogStream::Stdout;
        let json = serde_json::to_string(&stdout).unwrap();
        let parsed: LogStream = serde_json::from_str(&json).unwrap();
        assert_eq!(stdout, parsed);

        let stderr = LogStream::Stderr;
        let json = serde_json::to_string(&stderr).unwrap();
        let parsed: LogStream = serde_json::from_str(&json).unwrap();
        assert_eq!(stderr, parsed);
    }

    #[test]
    fn test_process_summary_with_all_fields() {
        let log = LogLine {
            timestamp: Utc::now(),
            stream: LogStream::Stdout,
            content: "log message".to_string(),
        };

        let summary = ProcessSummary {
            id: "full-summary".to_string(),
            command: "cargo".to_string(),
            args: vec!["run".to_string(), "--release".to_string()],
            status: ProcessStatus::Crashed { exit_code: Some(1) },
            pid: Some(54321),
            started_at: Some(Utc::now()),
            uptime_secs: Some(3600),
            health_matched: false,
            restart_count: 2,
            expected_port: Some(9000),
            recent_logs: vec![log],
        };

        let json = serde_json::to_string(&summary).unwrap();
        assert!(json.contains("full-summary"));
        assert!(json.contains("crashed"));
        assert!(json.contains("9000"));
        assert!(json.contains("log message"));
    }

    #[test]
    fn test_managed_process_new_initial_state() {
        let config = ProcessConfig {
            id: "initial".to_string(),
            command: "test".to_string(),
            args: vec![],
            cwd: None,
            env: HashMap::new(),
            health_check_pattern: None,
            health_check_timeout_secs: None,
            expected_port: None,
            auto_restart: false,
            max_restart_attempts: 0,
        };

        let proc = ManagedProcess::new(config);

        assert_eq!(proc.status, ProcessStatus::Stopped);
        assert!(proc.pid.is_none());
        assert!(proc.started_at.is_none());
        assert!(proc.log_buffer.is_empty());
        assert!(!proc.health_matched);
        assert_eq!(proc.restart_count, 0);
        assert!(proc.child_handle.is_none());
    }

    #[test]
    fn test_managed_process_add_log_alternating_streams() {
        let config = ProcessConfig {
            id: "alt".to_string(),
            command: "test".to_string(),
            args: vec![],
            cwd: None,
            env: HashMap::new(),
            health_check_pattern: None,
            health_check_timeout_secs: None,
            expected_port: None,
            auto_restart: false,
            max_restart_attempts: 0,
        };

        let mut proc = ManagedProcess::new(config);
        proc.add_log(LogStream::Stdout, "out1".to_string());
        proc.add_log(LogStream::Stderr, "err1".to_string());
        proc.add_log(LogStream::Stdout, "out2".to_string());
        proc.add_log(LogStream::Stderr, "err2".to_string());

        assert_eq!(proc.log_buffer.len(), 4);
        assert_eq!(proc.log_buffer[0].stream, LogStream::Stdout);
        assert_eq!(proc.log_buffer[1].stream, LogStream::Stderr);
        assert_eq!(proc.log_buffer[2].stream, LogStream::Stdout);
        assert_eq!(proc.log_buffer[3].stream, LogStream::Stderr);
    }

    #[test]
    fn test_managed_process_to_summary_empty_logs() {
        let config = ProcessConfig {
            id: "empty".to_string(),
            command: "test".to_string(),
            args: vec![],
            cwd: None,
            env: HashMap::new(),
            health_check_pattern: None,
            health_check_timeout_secs: None,
            expected_port: None,
            auto_restart: false,
            max_restart_attempts: 0,
        };

        let proc = ManagedProcess::new(config);
        let summary = proc.to_summary(100);

        assert!(summary.recent_logs.is_empty());
        assert!(summary.uptime_secs.is_none());
    }

    #[test]
    fn test_managed_process_to_summary_request_more_logs_than_available() {
        let config = ProcessConfig {
            id: "few-logs".to_string(),
            command: "test".to_string(),
            args: vec![],
            cwd: None,
            env: HashMap::new(),
            health_check_pattern: None,
            health_check_timeout_secs: None,
            expected_port: None,
            auto_restart: false,
            max_restart_attempts: 0,
        };

        let mut proc = ManagedProcess::new(config);
        proc.add_log(LogStream::Stdout, "line1".to_string());
        proc.add_log(LogStream::Stdout, "line2".to_string());

        let summary = proc.to_summary(100); // Request 100 but only 2 available
        assert_eq!(summary.recent_logs.len(), 2);
    }

    #[tokio::test]
    async fn test_is_port_available_high_port() {
        // Bind to port 0 to get an OS-assigned free port, then release it and
        // verify is_port_available() agrees it's free.  This avoids hardcoded
        // port collisions when many tests run in parallel.
        //
        // Note: There is an inherent TOCTOU race between dropping the listener
        // and probing the port.  We retry a few times to tolerate OS timing.
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0u16))
            .await
            .expect("Should bind to ephemeral port");
        let port = listener.local_addr().unwrap().port();
        drop(listener); // release port

        // Give the OS a moment to fully release the socket
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        let mut available = false;
        for _ in 0..3 {
            if is_port_available(port).await {
                available = true;
                break;
            }
            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        }
        assert!(
            available,
            "Recently released port {} should be available after retries",
            port
        );
    }

    #[tokio::test(start_paused = true)]
    async fn test_find_available_port_narrow_range() {
        // Use a range of high ephemeral ports unlikely to be in use
        let port = find_available_port(59100, 59200).await;
        // Should find at least one free port in a 100-port range
        assert!(port.is_some());
        assert!(port.unwrap() >= 59100 && port.unwrap() <= 59200);
    }

    #[tokio::test(start_paused = true)]
    async fn test_find_available_port_all_used() {
        // Bind a port, then search only that port range
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();

        // Search for just that port (which is in use)
        let result = find_available_port(port, port).await;
        assert!(result.is_none());
    }

    #[tokio::test(start_paused = true)]
    #[cfg(unix)]
    async fn test_port_info_unused_port() {
        // Port info for an unused high port
        let info = port_info(59998).await;
        // Should return None since nothing is listening
        assert!(info.is_none() || info.as_ref().map(|s| s.is_empty()).unwrap_or(true));
    }

    #[tokio::test(start_paused = true)]
    async fn test_process_manager_multiple_processes() {
        let manager = ProcessManager::new();

        let config1 = ProcessConfig {
            id: "multi-1".to_string(),
            command: "sleep".to_string(),
            args: vec!["0.5".to_string()],
            cwd: None,
            env: HashMap::new(),
            health_check_pattern: None,
            health_check_timeout_secs: None,
            expected_port: None,
            auto_restart: false,
            max_restart_attempts: 0,
        };

        let config2 = ProcessConfig {
            id: "multi-2".to_string(),
            command: "sleep".to_string(),
            args: vec!["0.5".to_string()],
            cwd: None,
            env: HashMap::new(),
            health_check_pattern: None,
            health_check_timeout_secs: None,
            expected_port: None,
            auto_restart: false,
            max_restart_attempts: 0,
        };

        let _ = manager.start(config1).await;
        let _ = manager.start(config2).await;

        let list = manager.list().await;
        assert_eq!(list.len(), 2);
        assert!(list.iter().any(|p| p.id == "multi-1"));
        assert!(list.iter().any(|p| p.id == "multi-2"));
    }

    #[tokio::test]
    async fn test_process_manager_stderr_capture() {
        let manager = ProcessManager::new();

        let config = ProcessConfig {
            id: "stderr-test".to_string(),
            command: "sh".to_string(),
            args: vec!["-c".to_string(), "echo error >&2".to_string()],
            cwd: None,
            env: HashMap::new(),
            health_check_pattern: None,
            health_check_timeout_secs: None,
            expected_port: None,
            auto_restart: false,
            max_restart_attempts: 0,
        };

        let _ = manager.start(config).await;
        tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

        let logs = manager.logs("stderr-test", 10).await.unwrap();
        assert!(logs
            .iter()
            .any(|l| l.stream == LogStream::Stderr && l.content.contains("error")));
    }

    #[test]
    fn test_process_config_env_multiple_vars() {
        let mut env = HashMap::new();
        env.insert("VAR1".to_string(), "value1".to_string());
        env.insert("VAR2".to_string(), "value2".to_string());
        env.insert("VAR3".to_string(), "value3".to_string());

        let config = ProcessConfig {
            id: "env-multi".to_string(),
            command: "test".to_string(),
            args: vec![],
            cwd: None,
            env: env.clone(),
            health_check_pattern: None,
            health_check_timeout_secs: None,
            expected_port: None,
            auto_restart: false,
            max_restart_attempts: 0,
        };

        assert_eq!(config.env.len(), 3);
        assert_eq!(config.env.get("VAR1"), Some(&"value1".to_string()));
        assert_eq!(config.env.get("VAR2"), Some(&"value2".to_string()));
        assert_eq!(config.env.get("VAR3"), Some(&"value3".to_string()));
    }

    #[test]
    fn test_max_log_lines_constant() {
        // Verify the constant value
        assert_eq!(MAX_LOG_LINES, 500);
    }

    #[test]
    fn test_health_check_timeout_constant() {
        // Verify the constant value
        assert_eq!(HEALTH_CHECK_TIMEOUT_SECS, 60);
    }

    #[test]
    fn test_process_status_partial_eq() {
        let running1 = ProcessStatus::Running;
        let running2 = ProcessStatus::Running;
        assert!(running1 == running2);

        let crashed1 = ProcessStatus::Crashed { exit_code: Some(1) };
        let crashed2 = ProcessStatus::Crashed { exit_code: Some(1) };
        assert!(crashed1 == crashed2);

        let restarting1 = ProcessStatus::Restarting { attempt: 3 };
        let restarting2 = ProcessStatus::Restarting { attempt: 3 };
        assert!(restarting1 == restarting2);

        let restarting3 = ProcessStatus::Restarting { attempt: 4 };
        assert!(restarting1 != restarting3);
    }

    #[test]
    fn test_log_line_partial_eq() {
        let timestamp = Utc::now();
        let log1 = LogLine {
            timestamp,
            stream: LogStream::Stdout,
            content: "test".to_string(),
        };
        let log2 = LogLine {
            timestamp,
            stream: LogStream::Stdout,
            content: "test".to_string(),
        };
        // LogLine doesn't derive PartialEq but we can compare fields
        assert_eq!(log1.stream, log2.stream);
        assert_eq!(log1.content, log2.content);
    }

    #[test]
    fn test_process_config_args_multiple() {
        let config = ProcessConfig {
            id: "multi-args".to_string(),
            command: "cargo".to_string(),
            args: vec![
                "test".to_string(),
                "--lib".to_string(),
                "--".to_string(),
                "--nocapture".to_string(),
            ],
            cwd: None,
            env: HashMap::new(),
            health_check_pattern: None,
            health_check_timeout_secs: None,
            expected_port: None,
            auto_restart: false,
            max_restart_attempts: 0,
        };

        assert_eq!(config.args.len(), 4);
        assert_eq!(config.args[0], "test");
        assert_eq!(config.args[3], "--nocapture");
    }

    #[tokio::test(start_paused = true)]
    async fn test_process_manager_invalid_command() {
        let manager = ProcessManager::new();

        let config = ProcessConfig {
            id: "invalid-cmd".to_string(),
            command: "nonexistent_command_xyz_123".to_string(),
            args: vec![],
            cwd: None,
            env: HashMap::new(),
            health_check_pattern: None,
            health_check_timeout_secs: None,
            expected_port: None,
            auto_restart: false,
            max_restart_attempts: 0,
        };

        let result = manager.start(config).await;
        assert!(result.is_err());
    }

    #[test]
    fn test_process_summary_with_restarting_status() {
        let summary = ProcessSummary {
            id: "restarting-summary".to_string(),
            command: "server".to_string(),
            args: vec![],
            status: ProcessStatus::Restarting { attempt: 3 },
            pid: None,
            started_at: Some(Utc::now()),
            uptime_secs: None,
            health_matched: false,
            restart_count: 3,
            expected_port: Some(8080),
            recent_logs: vec![],
        };

        let json = serde_json::to_string(&summary).unwrap();
        assert!(json.contains("restarting"));
        assert!(json.contains("attempt"));
    }

    #[test]
    fn test_process_config_cwd_pathbuf() {
        let config = ProcessConfig {
            id: "pathbuf".to_string(),
            command: "ls".to_string(),
            args: vec![],
            cwd: Some(PathBuf::from("/home/user/project/src")),
            env: HashMap::new(),
            health_check_pattern: None,
            health_check_timeout_secs: None,
            expected_port: None,
            auto_restart: false,
            max_restart_attempts: 0,
        };

        assert!(config.cwd.is_some());
        assert_eq!(config.cwd.unwrap().to_str(), Some("/home/user/project/src"));
    }

    /// A process that exits with a non-zero code immediately should return an error
    /// from start() instead of silently reporting success.
    #[tokio::test]
    #[cfg(not(target_os = "windows"))]
    async fn test_start_returns_error_when_process_exits_with_nonzero() {
        let manager = ProcessManager::new();

        let config = ProcessConfig {
            id: "exit-fail-test".to_string(),
            command: "sh".to_string(),
            args: vec!["-c".to_string(), "exit 42".to_string()],
            cwd: None,
            env: HashMap::new(),
            health_check_pattern: None,
            health_check_timeout_secs: None,
            expected_port: None,
            auto_restart: false,
            max_restart_attempts: 0,
        };

        let result = manager.start(config).await;
        assert!(
            result.is_err(),
            "start() should return Err when process exits immediately with non-zero code"
        );
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("exited immediately") || err_msg.contains("42"),
            "Error should mention exit code. Got: {}",
            err_msg
        );
    }

    /// A process with a health check that times out should return an error.
    #[tokio::test]
    #[cfg(not(target_os = "windows"))]
    async fn test_start_returns_error_on_health_check_timeout() {
        let manager = ProcessManager::new();

        // Process that runs but never prints the health check pattern
        let config = ProcessConfig {
            id: "health-timeout-test".to_string(),
            command: "sleep".to_string(),
            args: vec!["60".to_string()],
            cwd: None,
            env: HashMap::new(),
            health_check_pattern: Some("READY_PATTERN_THAT_NEVER_APPEARS".to_string()),
            health_check_timeout_secs: Some(1), // 1 second timeout
            expected_port: None,
            auto_restart: false,
            max_restart_attempts: 0,
        };

        let result = manager.start(config).await;
        assert!(
            result.is_err(),
            "start() should return Err when health check times out"
        );
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("health check timed out"),
            "Error should mention health check timeout. Got: {}",
            err_msg
        );

        // Cleanup
        let _ = manager.stop("health-timeout-test", true).await;
    }

    /// A process that exits during health check wait should return an error
    /// with the exit code.
    #[tokio::test]
    #[cfg(not(target_os = "windows"))]
    async fn test_start_returns_error_when_process_crashes_during_health_check() {
        let manager = ProcessManager::new();

        let config = ProcessConfig {
            id: "crash-health-test".to_string(),
            command: "sh".to_string(),
            args: vec!["-c".to_string(), "echo 'starting up'; exit 7".to_string()],
            cwd: None,
            env: HashMap::new(),
            health_check_pattern: Some("READY".to_string()),
            health_check_timeout_secs: Some(3),
            expected_port: None,
            auto_restart: false,
            max_restart_attempts: 0,
        };

        let result = manager.start(config).await;
        assert!(
            result.is_err(),
            "start() should return Err when process crashes during health check wait"
        );
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("exited immediately") || err_msg.contains("7"),
            "Error should mention exit code. Got: {}",
            err_msg
        );
    }

    /// Spawning a nonexistent command should return an error from start().
    #[tokio::test]
    async fn test_start_returns_error_for_nonexistent_command() {
        let manager = ProcessManager::new();

        let config = ProcessConfig {
            id: "no-such-cmd".to_string(),
            command: "this_command_surely_does_not_exist_anywhere_12345".to_string(),
            args: vec![],
            cwd: None,
            env: HashMap::new(),
            health_check_pattern: None,
            health_check_timeout_secs: None,
            expected_port: None,
            auto_restart: false,
            max_restart_attempts: 0,
        };

        let result = manager.start(config).await;
        assert!(
            result.is_err(),
            "start() should return Err for nonexistent command"
        );
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("Failed to spawn"),
            "Error should mention spawn failure. Got: {}",
            err_msg
        );
    }

    /// After health check timeout, the process status should be HealthCheckFailed
    /// (not Starting or Running).
    #[tokio::test]
    #[cfg(not(target_os = "windows"))]
    async fn test_health_check_timeout_sets_failed_status() {
        let manager = ProcessManager::new();

        let config = ProcessConfig {
            id: "status-check-test".to_string(),
            command: "sleep".to_string(),
            args: vec!["60".to_string()],
            cwd: None,
            env: HashMap::new(),
            health_check_pattern: Some("WILL_NEVER_MATCH".to_string()),
            health_check_timeout_secs: Some(1),
            expected_port: None,
            auto_restart: false,
            max_restart_attempts: 0,
        };

        // start() returns error, but the process entry still exists
        let _ = manager.start(config).await;

        // Verify the stored status is HealthCheckFailed
        let get_result = manager.get("status-check-test").await;
        assert!(get_result.is_ok());
        let summary = get_result.unwrap();
        assert!(
            matches!(summary.status, ProcessStatus::HealthCheckFailed),
            "Status should be HealthCheckFailed but was {:?}",
            summary.status
        );

        // Cleanup
        let _ = manager.stop("status-check-test", true).await;
    }

    /// Process that exits with code 0 immediately (no health check) should
    /// report crashed status since it didn't stay alive.
    #[tokio::test]
    #[cfg(not(target_os = "windows"))]
    async fn test_start_returns_error_when_process_exits_zero_immediately() {
        let manager = ProcessManager::new();

        let config = ProcessConfig {
            id: "exit-zero-test".to_string(),
            command: "true".to_string(),
            args: vec![],
            cwd: None,
            env: HashMap::new(),
            health_check_pattern: None,
            health_check_timeout_secs: None,
            expected_port: None,
            auto_restart: false,
            max_restart_attempts: 0,
        };

        let result = manager.start(config).await;
        // Even exit code 0 means the process didn't stay running, which is
        // a failure for a background process manager.
        assert!(
            result.is_err(),
            "start() should return Err when process exits immediately even with code 0"
        );
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("exited immediately"),
            "Error should mention immediate exit. Got: {}",
            err_msg
        );
    }
}
