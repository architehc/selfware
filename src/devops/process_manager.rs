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
#[path = "../../tests/unit/devops/process_manager/process_manager_test.rs"]
mod tests;
