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

use crate::safety::process_env::SanitizedEnvExt;
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::process::{ExitStatus, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Weak};
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
    /// Identifies this installation of an id so stale monitor/output tasks
    /// cannot mutate a later process that reused the same id.
    generation: u64,
    /// Identifies the child currently installed within one auto-restarting
    /// generation so buffered output from an exited child cannot mark its
    /// replacement healthy.
    incarnation: u64,
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
            generation: 0,
            incarnation: 0,
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

fn is_expected_manual_restart(proc: &ManagedProcess, generation: Option<u64>) -> bool {
    generation.is_some_and(|generation| {
        proc.generation == generation
            && matches!(proc.status, ProcessStatus::Restarting { attempt: 0 })
    })
}

/// Manager for background processes
pub struct ProcessManager {
    processes: Arc<RwLock<HashMap<String, ManagedProcess>>>,
    port_reservations: Arc<Mutex<HashMap<u16, PortReservation>>>,
    start_locks: Mutex<HashMap<String, Weak<Mutex<()>>>>,
    next_generation: AtomicU64,
}

impl ProcessManager {
    pub fn new() -> Self {
        Self {
            processes: Arc::new(RwLock::new(HashMap::new())),
            port_reservations: Arc::new(Mutex::new(HashMap::new())),
            start_locks: Mutex::new(HashMap::new()),
            next_generation: AtomicU64::new(1),
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
        drop(processes);

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
            let (child_handle, pid, generation, status) = {
                let processes = self.processes.read().await;
                let Some(proc) = processes.get(&id) else {
                    continue;
                };
                // The monitor deliberately keeps the exited child's handle while
                // it owns an auto-restart backoff. Reaping/clearing that state
                // here lets a second reconcile misclassify the pending restart as
                // orphaned and cancel it.
                if matches!(proc.status, ProcessStatus::Restarting { .. }) {
                    continue;
                }
                (
                    proc.child_handle.clone(),
                    proc.pid,
                    proc.generation,
                    proc.status.clone(),
                )
            };

            // A live child is owned by its monitor. Reconcile must not reap
            // the exit first or the monitor will see an empty handle and lose
            // the auto-restart transition.
            if matches!(status, ProcessStatus::Running | ProcessStatus::Starting) {
                if let Some(handle) = &child_handle {
                    if handle.read().await.is_some() {
                        continue;
                    }
                }
            }

            let mut observed_exit_code = None;
            let mut cleared_handle = false;
            let mut missing_running_handle = false;

            if let Some(handle) = child_handle {
                let mut child_guard = handle.write().await;
                if let Some(child) = child_guard.as_mut() {
                    if let Some(status) = try_wait_managed(child).ok().flatten() {
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
            if let Some(proc) = processes
                .get_mut(&id)
                .filter(|proc| proc.generation == generation && proc.pid == pid)
            {
                if let Some(exit_code) = observed_exit_code {
                    report.exited_processes += 1;
                    if cleared_handle {
                        report.handles_cleared += 1;
                    }
                    proc.child_handle = None;
                    proc.pid = None;
                    if !matches!(
                        proc.status,
                        ProcessStatus::Stopped
                            | ProcessStatus::HealthCheckFailed
                            | ProcessStatus::Restarting { .. }
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
        self.start_inner(config, None, None).await
    }

    /// Start a managed process and register it with the task/session resource
    /// owner as soon as the child exists. This closes the startup window where
    /// cancellation or an early crash could leave an auto-restarted child
    /// outside resource teardown.
    pub async fn start_tracked(&self, config: ProcessConfig, keep: bool) -> Result<ProcessSummary> {
        self.start_inner(config, Some(keep), None).await
    }

    async fn start_inner(
        &self,
        config: ProcessConfig,
        track_keep: Option<bool>,
        expected_restart_generation: Option<u64>,
    ) -> Result<ProcessSummary> {
        let id = config.id.clone();

        let health_pattern = config
            .health_check_pattern
            .as_ref()
            .map(|p| Regex::new(p))
            .transpose()
            .context("Invalid health check regex pattern")?;

        let health_timeout = config
            .health_check_timeout_secs
            .unwrap_or(HEALTH_CHECK_TIMEOUT_SECS);

        // Serialize the complete startup protocol per id, including port
        // acquisition and readiness. A concurrent identical request waits
        // and then reuses the first child; a different configuration gets a
        // deterministic conflict instead of racing two spawns.
        let start_lock = {
            let mut locks = self.start_locks.lock().await;
            locks.retain(|_, lock| lock.strong_count() > 0);
            if let Some(lock) = locks.get(&id).and_then(Weak::upgrade) {
                lock
            } else {
                let lock = Arc::new(Mutex::new(()));
                locks.insert(id.clone(), Arc::downgrade(&lock));
                lock
            }
        };
        let _start_guard = start_lock.lock().await;

        {
            let processes = self.processes.read().await;
            if let Some(existing) = processes.get(&id) {
                let expected_restart =
                    is_expected_manual_restart(existing, expected_restart_generation);
                if expected_restart_generation.is_some() && !expected_restart {
                    anyhow::bail!(
                        "Process '{}' manual restart was cancelled or superseded",
                        id
                    );
                }
                if expected_restart_generation.is_none()
                    && matches!(existing.status, ProcessStatus::Restarting { attempt: 0 })
                {
                    anyhow::bail!("Process '{}' is being restarted", id);
                }
                if !expected_restart
                    && matches!(
                        existing.status,
                        ProcessStatus::Running
                            | ProcessStatus::Starting
                            | ProcessStatus::Restarting { .. }
                    )
                {
                    if existing.config != config {
                        anyhow::bail!(
                            "Process '{}' is already running with a different configuration",
                            id
                        );
                    }
                    let summary = existing.to_summary(50);
                    if let (Some(keep), Some(pid)) = (track_keep, summary.pid) {
                        record_managed_process(&config, pid, keep);
                    }
                    info!("Reusing existing managed process '{}'", id);
                    return Ok(summary);
                }
            } else if expected_restart_generation.is_some() {
                anyhow::bail!("Process '{}' disappeared while it was restarting", id);
            }
        }

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
        // SELFWARE_API_KEY) into child processes, then re-add the shared
        // non-sensitive allowlist (see safety::process_env). This matches
        // spawn_child_process (the restart path already does this); the
        // initial spawn must be consistent.
        crate::safety::process_env::sanitize_command_env(&mut cmd);
        for (key, value) in &config.env {
            cmd.env(key, value);
        }
        apply_git_hardening(&mut cmd, &config);

        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());
        cmd.kill_on_drop(true); // Ensure child is killed if handle is dropped to prevent zombies
                                // Lead its own process group so stop/teardown reach the whole tree
                                // (`npm run dev` → node → esbuild), not just the direct child.
        #[cfg(unix)]
        cmd.process_group(0);

        info!(
            "Starting process '{}': {} {:?}",
            id, config.command, config.args
        );

        // Hold the id map's write lock across the synchronous spawn and
        // insertion. Two concurrent starts of the same id can no longer both
        // create children and overwrite one handle with the other.
        let (pid, child_handle, generation, reused) = {
            let mut processes = self.processes.write().await;
            if let Some(existing) = processes.get(&id) {
                let expected_restart =
                    is_expected_manual_restart(existing, expected_restart_generation);
                if expected_restart_generation.is_some() && !expected_restart {
                    anyhow::bail!(
                        "Process '{}' manual restart was cancelled or superseded",
                        id
                    );
                }
                if expected_restart_generation.is_none()
                    && matches!(existing.status, ProcessStatus::Restarting { attempt: 0 })
                {
                    anyhow::bail!("Process '{}' is being restarted", id);
                }
                if !expected_restart
                    && matches!(
                        existing.status,
                        ProcessStatus::Running
                            | ProcessStatus::Starting
                            | ProcessStatus::Restarting { .. }
                    )
                {
                    if existing.config == config {
                        info!("Reusing existing managed process '{}'", id);
                        (
                            existing.pid,
                            existing
                                .child_handle
                                .clone()
                                .context("active process has no child handle")?,
                            existing.generation,
                            Some(existing.to_summary(50)),
                        )
                    } else {
                        anyhow::bail!(
                            "Process '{}' is already running with a different configuration",
                            id
                        );
                    }
                } else {
                    let generation = self.next_generation.fetch_add(1, Ordering::Relaxed);
                    drop(reserved_port_listener);
                    let child = cmd.spawn().with_context(|| {
                        format!(
                            "Failed to spawn process: {} {:?}",
                            config.command, config.args
                        )
                    })?;
                    let pid = child.id();
                    let child_handle = Arc::new(RwLock::new(Some(child)));
                    let mut managed = ManagedProcess::new(config.clone());
                    managed.status = ProcessStatus::Starting;
                    managed.pid = pid;
                    managed.started_at = Some(Utc::now());
                    managed.generation = generation;
                    managed.child_handle = Some(child_handle.clone());
                    processes.insert(id.clone(), managed);
                    (pid, child_handle, generation, None)
                }
            } else {
                if expected_restart_generation.is_some() {
                    anyhow::bail!("Process '{}' disappeared while it was restarting", id);
                }
                let generation = self.next_generation.fetch_add(1, Ordering::Relaxed);
                drop(reserved_port_listener);
                let child = cmd.spawn().with_context(|| {
                    format!(
                        "Failed to spawn process: {} {:?}",
                        config.command, config.args
                    )
                })?;
                let pid = child.id();
                let child_handle = Arc::new(RwLock::new(Some(child)));
                let mut managed = ManagedProcess::new(config.clone());
                managed.status = ProcessStatus::Starting;
                managed.pid = pid;
                managed.started_at = Some(Utc::now());
                managed.generation = generation;
                managed.child_handle = Some(child_handle.clone());
                processes.insert(id.clone(), managed);
                (pid, child_handle, generation, None)
            }
        };

        if let Some(summary) = reused {
            if let (Some(keep), Some(pid)) = (track_keep, summary.pid) {
                record_managed_process(&config, pid, keep);
            }
            return Ok(summary);
        }
        if let (Some(keep), Some(pid)) = (track_keep, pid) {
            record_managed_process(&config, pid, keep);
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
                        generation,
                        0,
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
                    collect_output(
                        processes,
                        id,
                        generation,
                        0,
                        stderr,
                        LogStream::Stderr,
                        None,
                    )
                    .await;
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
                generation,
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
                            let current_pid = child.id().or(pid);
                            if let Some(status) = try_wait_managed(&mut child).ok().flatten() {
                                (status.code(), false)
                            } else {
                                warn!(
                                    "Process '{}' failed health check and will be terminated",
                                    id
                                );
                                let exit_code =
                                    force_kill_process_tree(&mut child, current_pid).await;
                                (exit_code, true)
                            }
                        } else {
                            (None, false)
                        }
                    };

                    let mut processes = self.processes.write().await;
                    if let Some(proc) = processes
                        .get_mut(&id)
                        .filter(|proc| proc.generation == generation)
                    {
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
                    if let Some(proc) = processes
                        .get(&id)
                        .filter(|proc| proc.generation == generation)
                    {
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
            // poll the monitor-owned status throughout it instead of taking a
            // single snapshot at the end. The startup path must not compete
            // with the monitor to reap an immediately-crashing child.
            let deadline = tokio::time::Instant::now() + tokio::time::Duration::from_millis(800);
            loop {
                // The monitor is the sole exit observer. If startup reaped
                // the child too, reconciliation could prune the transient
                // terminal record before the monitor publishes auto-restart.
                let monitor_saw_exit = {
                    let processes = self.processes.read().await;
                    processes.get(&id).is_none_or(|p| {
                        p.generation != generation
                            || matches!(
                                p.status,
                                ProcessStatus::Crashed { .. }
                                    | ProcessStatus::Stopped
                                    | ProcessStatus::Restarting { .. }
                            )
                    })
                };
                if monitor_saw_exit || tokio::time::Instant::now() >= deadline {
                    break;
                }
                tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
            }

            let mut processes = self.processes.write().await;
            if let Some(proc) = processes
                .get_mut(&id)
                .filter(|proc| proc.generation == generation)
            {
                if matches!(proc.status, ProcessStatus::Starting) {
                    // The monitor saw no exit within the startup window.
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
            .filter(|proc| proc.generation == generation)
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
        self.stop_with_status(id, force, ProcessStatus::Stopped, None)
            .await
    }

    async fn stop_for_restart(&self, id: &str, generation: u64) -> Result<ProcessSummary> {
        self.stop_with_status(
            id,
            false,
            ProcessStatus::Restarting { attempt: 0 },
            Some(generation),
        )
        .await
    }

    async fn stop_with_status(
        &self,
        id: &str,
        force: bool,
        final_status: ProcessStatus,
        expected_generation: Option<u64>,
    ) -> Result<ProcessSummary> {
        let (child_handle, pid, generation, mut stopped_summary) = {
            let mut processes = self.processes.write().await;
            let proc = processes
                .get_mut(id)
                .ok_or_else(|| anyhow::anyhow!("Process '{}' not found", id))?;

            if expected_generation.is_some_and(|generation| proc.generation != generation) {
                anyhow::bail!("Process '{}' changed while it was restarting", id);
            }

            if matches!(
                proc.status,
                ProcessStatus::Stopped | ProcessStatus::Crashed { .. }
            ) && matches!(&final_status, ProcessStatus::Stopped)
            {
                return Ok(proc.to_summary(20));
            }

            info!("Stopping process '{}' (force={})", id, force);
            // Publish the terminal/restart intent before awaiting child
            // shutdown. The monitor cannot auto-restart it, and resource
            // teardown can turn a manual Restarting marker into Stopped to
            // cancel the pending spawn.
            proc.status = final_status;
            (
                proc.child_handle.clone(),
                proc.pid,
                proc.generation,
                proc.to_summary(20),
            )
        };

        // Child shutdown can take the full graceful timeout. Keep that wait
        // outside the process-map lock so unrelated get/list/start/stop calls
        // remain available.
        if let Some(child_handle) = child_handle {
            let mut child_guard = child_handle.write().await;
            if let Some(ref mut child) = *child_guard {
                let already_exited = try_wait_managed(child)
                    .with_context(|| format!("inspect process '{id}' before stop"))?
                    .is_some();
                let pid = child.id().or(pid);
                if !already_exited {
                    if force {
                        let _ = force_kill_process_tree(child, pid).await;
                    } else {
                        // Try graceful shutdown first
                        #[cfg(unix)]
                        {
                            use nix::sys::signal::{kill, Signal};
                            use nix::unistd::Pid;
                            if let Some(pid) = pid {
                                if let Ok(raw_pid) = i32::try_from(pid) {
                                    // The child leads its own group (see
                                    // `start`): SIGTERM the whole tree, falling
                                    // back to the pid alone.
                                    if nix::sys::signal::killpg(
                                        Pid::from_raw(raw_pid),
                                        Signal::SIGTERM,
                                    )
                                    .is_err()
                                    {
                                        let _ = kill(Pid::from_raw(raw_pid), Signal::SIGTERM);
                                    }
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
                    }
                    #[cfg(not(unix))]
                    {
                        let _ = child.kill().await;
                        let _ = child.wait().await;
                    }

                    // Poll without reaping first so an exited leader keeps its
                    // pid pinned until `try_wait_managed` kills any
                    // TERM-ignoring descendants in the group.
                    let deadline = tokio::time::Instant::now() + Duration::from_secs(3);
                    loop {
                        match try_wait_managed(child) {
                            Ok(Some(_)) => break,
                            Ok(None) if tokio::time::Instant::now() < deadline => {
                                tokio::time::sleep(Duration::from_millis(25)).await;
                            }
                            Ok(None) | Err(_) => {
                                warn!(
                                    "Process '{}' did not exit after SIGTERM, sending SIGKILL",
                                    id
                                );
                                let _ = force_kill_process_tree(child, pid).await;
                                break;
                            }
                        }
                    }
                }
            }
        }

        let mut processes = self.processes.write().await;
        if let Some(proc) = processes
            .get_mut(id)
            .filter(|proc| proc.generation == generation)
        {
            // Do not overwrite status here: signal() may have changed a
            // Restarting marker to Stopped while shutdown was in flight.
            proc.pid = None;
            return Ok(proc.to_summary(20));
        }

        // A remove followed by a new start may replace this generation while
        // its old child is shutting down. Report the completed stop without
        // mutating the replacement record.
        stopped_summary.pid = None;
        Ok(stopped_summary)
    }

    /// Signal a managed process (its whole process group) without waiting:
    /// SIGTERM, or SIGKILL with `force`. The entry is marked `Stopped` first
    /// so the monitor never auto-restarts a process being torn down.
    /// Returns `Ok(false)` when `id` is not managed here.
    pub async fn signal(&self, id: &str, force: bool) -> Result<bool> {
        let handle = {
            let mut processes = self.processes.write().await;
            let Some(proc) = processes.get_mut(id) else {
                return Ok(false);
            };
            proc.status = ProcessStatus::Stopped;
            proc.child_handle.clone()
        };
        let Some(handle) = handle else {
            return Ok(true);
        };
        let mut guard = handle.write().await;
        let Some(child) = guard.as_mut() else {
            return Ok(true);
        };
        if matches!(try_wait_managed(child), Ok(Some(_))) {
            return Ok(true);
        }
        // The child is unreaped, so its pid (and the group it leads) cannot
        // have been reused.
        #[cfg(unix)]
        if let Some(pid) = child.id().and_then(|p| i32::try_from(p).ok()) {
            use nix::sys::signal::{kill, killpg, Signal};
            use nix::unistd::Pid;
            let signal = if force {
                Signal::SIGKILL
            } else {
                Signal::SIGTERM
            };
            if killpg(Pid::from_raw(pid), signal).is_err() {
                kill(Pid::from_raw(pid), signal)
                    .with_context(|| format!("signal process '{id}'"))?;
            }
            return Ok(true);
        }
        child
            .start_kill()
            .with_context(|| format!("kill process '{id}'"))?;
        Ok(true)
    }

    /// Whether the managed process has exited (reaping it if so). `None`
    /// when `id` is unknown here or its state cannot be read.
    pub async fn has_exited(&self, id: &str) -> Option<bool> {
        let handle = {
            let processes = self.processes.read().await;
            let proc = processes.get(id)?;
            // An active lifecycle state is authoritative for teardown. In
            // particular Restarting has no live child during backoff, but must be
            // reported Running so the resource driver calls signal(), marks it
            // Stopped, and cancels the pending restart before releasing it.
            if matches!(
                proc.status,
                ProcessStatus::Running | ProcessStatus::Starting | ProcessStatus::Restarting { .. }
            ) {
                return Some(false);
            }
            proc.child_handle.clone()
        };
        let Some(handle) = handle else {
            return Some(true);
        };
        let mut guard = handle.write().await;
        let result = match guard.as_mut() {
            None => Some(true),
            Some(child) => match try_wait_managed(child) {
                Ok(Some(_)) => Some(true),
                Ok(None) => Some(false),
                Err(_) => None,
            },
        };
        result
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
            ProcessStatus::Running | ProcessStatus::Starting | ProcessStatus::Restarting { .. }
        ) {
            anyhow::bail!("Cannot remove running process '{}'. Stop it first.", id);
        }

        processes.remove(id);
        Ok(())
    }

    /// Restart a process
    pub async fn restart(&self, id: &str) -> Result<ProcessSummary> {
        self.restart_inner(id, None).await
    }

    /// Restart a tool-owned process while preserving its resource entry and
    /// refreshing that entry immediately after the replacement child exists.
    pub async fn restart_tracked(&self, id: &str) -> Result<ProcessSummary> {
        let keep = crate::resources::managed_process_entry(
            crate::resources::ResourceRegistry::global(),
            id,
        )
        .is_some_and(|entry| entry.keep);
        self.restart_inner(id, Some(keep)).await
    }

    async fn restart_inner(&self, id: &str, track_keep: Option<bool>) -> Result<ProcessSummary> {
        let (config, generation) = {
            let processes = self.processes.read().await;
            let proc = processes
                .get(id)
                .ok_or_else(|| anyhow::anyhow!("Process '{}' not found", id))?;
            (proc.config.clone(), proc.generation)
        };

        // Keep a monitor-visible Restarting marker for the whole gap between
        // children. Resource teardown sees it as active, calls signal(), and
        // changes it to Stopped; the expected-generation check in start_inner
        // then refuses to resurrect the process.
        self.stop_for_restart(id, generation).await?;
        let result = self.start_inner(config, track_keep, Some(generation)).await;
        if result.is_err() {
            let mut processes = self.processes.write().await;
            if let Some(proc) = processes.get_mut(id).filter(|proc| {
                proc.generation == generation
                    && matches!(proc.status, ProcessStatus::Restarting { attempt: 0 })
            }) {
                proc.status = ProcessStatus::Crashed { exit_code: None };
                proc.pid = None;
                proc.child_handle = None;
            }
        }
        result
    }
}

impl Default for ProcessManager {
    fn default() -> Self {
        Self::new()
    }
}

fn record_managed_process(config: &ProcessConfig, pid: u32, keep: bool) {
    use crate::resources::{NewResource, ResourceHandle, ResourceKind, ResourceRegistry};

    let registry = ResourceRegistry::global();
    let start_time = crate::resources::driver::process_start_time(pid);
    if crate::resources::managed_process_entry(registry, &config.id).is_some() {
        crate::resources::refresh_managed_process(registry, &config.id, pid, start_time);
        return;
    }

    let label = std::iter::once(config.command.as_str())
        .chain(config.args.iter().map(String::as_str))
        .collect::<Vec<_>>()
        .join(" ");
    registry.register(
        NewResource::new(
            ResourceKind::Process,
            ResourceHandle::Process {
                pid,
                pgid: cfg!(unix).then_some(pid),
                start_time,
                managed_id: Some(config.id.clone()),
            },
            format!("{}: {label}", config.id),
        )
        .keep(keep),
    );
}

/// Spawn a child process from config (used by start and restart)
/// git run by a managed process (directly or from its scripts) must not
/// execute what an untrusted repository configured
/// (`crate::safety::git_exec::shell_git_env`); the process's own env map
/// cannot carry git config variables past it.
fn apply_git_hardening(cmd: &mut Command, config: &ProcessConfig) {
    for key in config.env.keys() {
        if crate::safety::git_exec::is_git_config_env(key) {
            cmd.env_remove(key);
        }
    }
    let dir = config
        .cwd
        .as_ref()
        .map(std::path::PathBuf::from)
        .unwrap_or_else(crate::tools::workspace_root::current_path);
    crate::safety::git_exec::apply_shell_git_env(cmd, &dir);
}

async fn spawn_child_process(
    config: &ProcessConfig,
) -> Result<(Option<u32>, Arc<RwLock<Option<Child>>>)> {
    let mut cmd = Command::new(&config.command);
    cmd.args(&config.args);

    if let Some(ref cwd) = config.cwd {
        cmd.current_dir(cwd);
    }

    // Clear inherited environment to prevent secret leakage, then re-add the
    // shared non-sensitive allowlist (see safety::process_env).
    crate::safety::process_env::sanitize_command_env(&mut cmd);
    for (key, value) in &config.env {
        cmd.env(key, value);
    }
    apply_git_hardening(&mut cmd, config);

    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());
    cmd.kill_on_drop(true); // Ensure child is killed if handle is dropped to prevent zombies
                            // Same process-group rule as the initial spawn in `start`.
    #[cfg(unix)]
    cmd.process_group(0);

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

/// SIGKILL a process group that selfware created for a managed child. The
/// leader's pid is the pgid on Unix; other platforms fall back to Child::kill.
fn force_kill_process_group(pid: Option<u32>) {
    #[cfg(unix)]
    if let Some(raw) = pid.and_then(|pid| i32::try_from(pid).ok()) {
        if raw > 1 {
            let _ = nix::sys::signal::killpg(
                nix::unistd::Pid::from_raw(raw),
                nix::sys::signal::Signal::SIGKILL,
            );
        }
    }
    #[cfg(not(unix))]
    let _ = pid;
}

/// Observe a managed child exit without first releasing its pid, kill any
/// descendants in the process group while the zombie leader still pins that
/// identity, then reap the leader. A plain `Child::try_wait` followed by
/// `killpg(stored_pid)` can signal an unrelated group if the pid is reused in
/// between those operations.
fn try_wait_managed(child: &mut Child) -> std::io::Result<Option<ExitStatus>> {
    #[cfg(unix)]
    {
        let Some(pid) = child.id() else {
            return child.try_wait();
        };
        let mut info: libc::siginfo_t = unsafe { std::mem::zeroed() };
        let result = unsafe {
            libc::waitid(
                libc::P_PID,
                pid as libc::id_t,
                &mut info,
                libc::WEXITED | libc::WNOHANG | libc::WNOWAIT,
            )
        };
        if result != 0 {
            return Err(std::io::Error::last_os_error());
        }
        if unsafe { info.si_pid() } == 0 {
            return Ok(None);
        }
        force_kill_process_group(Some(pid));
        child.try_wait()
    }
    #[cfg(not(unix))]
    {
        child.try_wait()
    }
}

/// Kill the complete managed process tree and reap its direct child.
async fn force_kill_process_tree(child: &mut Child, _pid: Option<u32>) -> Option<i32> {
    force_kill_process_group(child.id());
    let _ = child.start_kill();
    child.wait().await.ok().and_then(|status| status.code())
}

/// Collect output from a process stream
async fn collect_output<R: tokio::io::AsyncRead + Unpin>(
    processes: Arc<RwLock<HashMap<String, ManagedProcess>>>,
    id: String,
    generation: u64,
    incarnation: u64,
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
                if let Some(proc) = procs
                    .get_mut(&id)
                    .filter(|proc| proc.generation == generation && proc.incarnation == incarnation)
                {
                    if !proc.health_matched && matches!(proc.status, ProcessStatus::Starting) {
                        proc.health_matched = true;
                        proc.status = ProcessStatus::Running;
                        info!("Process '{}' health check passed: {}", id, line);
                    }
                }
            }
        }

        // Store log line
        let mut procs = processes.write().await;
        if let Some(proc) = procs
            .get_mut(&id)
            .filter(|proc| proc.generation == generation && proc.incarnation == incarnation)
        {
            proc.add_log(stream.clone(), line);
        }
    }
}

/// Monitor a process for exit
async fn monitor_process(
    processes: Arc<RwLock<HashMap<String, ManagedProcess>>>,
    id: String,
    child_handle: Arc<RwLock<Option<Child>>>,
    generation: u64,
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
                    try_wait_managed(child).ok().flatten()
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
                if let Some(proc) = procs.get(&id).filter(|proc| proc.generation == generation) {
                    if matches!(proc.status, ProcessStatus::Stopped) {
                        break None;
                    }
                } else {
                    break None;
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
        if let Some(proc) = procs
            .get_mut(&id)
            .filter(|proc| proc.generation == generation)
        {
            let should_restart = auto_restart
                && (max_restarts == 0 || proc.restart_count < max_restarts)
                && !matches!(
                    proc.status,
                    ProcessStatus::Stopped | ProcessStatus::Restarting { .. }
                );

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

                // stop/remove/restart may have changed the record during the
                // backoff. Only this exact generation and attempt may spawn.
                let still_requested = {
                    let procs = processes.read().await;
                    procs.get(&id).is_some_and(|proc| {
                        proc.generation == generation
                            && matches!(
                                proc.status,
                                ProcessStatus::Restarting { attempt }
                                    if attempt == restart_attempt
                            )
                    })
                };
                if !still_requested {
                    break;
                }

                // Actually restart the process
                match spawn_child_process(&config).await {
                    Ok((pid, new_child_handle)) => {
                        let (mut spawned, stdout, stderr) = {
                            let mut guard = new_child_handle.write().await;
                            let mut spawned = guard.take().expect("spawn returned a child");
                            let stdout = spawned.stdout.take();
                            let stderr = spawned.stderr.take();
                            (Some(spawned), stdout, stderr)
                        };

                        // Hold the per-child lock while validating and
                        // installing the replacement. stop() publishes its
                        // intent in the process map before taking this lock,
                        // so it either cancels this install or kills the
                        // accepted replacement. Never await this child lock
                        // while retaining the global process-map lock.
                        let installed = {
                            let mut child_guard = child_handle.write().await;
                            let mut procs = processes.write().await;
                            match procs.get_mut(&id) {
                                Some(proc)
                                    if proc.generation == generation
                                        && matches!(
                                            proc.status,
                                            ProcessStatus::Restarting { attempt }
                                                if attempt == restart_attempt
                                        ) =>
                                {
                                    *child_guard = spawned.take();
                                    proc.pid = pid;
                                    proc.started_at = Some(Utc::now());
                                    proc.incarnation = u64::from(restart_attempt);
                                    proc.status = ProcessStatus::Starting;
                                    proc.health_matched = false;
                                    proc.child_handle = Some(child_handle.clone());
                                    true
                                }
                                _ => false,
                            }
                        };
                        if !installed {
                            if let Some(mut child) = spawned {
                                let _ = force_kill_process_tree(&mut child, pid).await;
                            }
                            break;
                        }

                        // The resource registry entry of a tool-started
                        // process still names the crashed pid: point it at
                        // the accepted replacement.
                        if let Some(pid) = pid {
                            crate::resources::refresh_managed_process(
                                crate::resources::ResourceRegistry::global(),
                                &id,
                                pid,
                                crate::resources::driver::process_start_time(pid),
                            );
                        }

                        if let Some(stdout) = stdout {
                            let procs = processes.clone();
                            let proc_id = id.clone();
                            let hp = health_pattern.clone();
                            tokio::spawn(async move {
                                collect_output(
                                    procs,
                                    proc_id,
                                    generation,
                                    u64::from(restart_attempt),
                                    stdout,
                                    LogStream::Stdout,
                                    hp,
                                )
                                .await;
                            });
                        }
                        if let Some(stderr) = stderr {
                            let procs = processes.clone();
                            let proc_id = id.clone();
                            tokio::spawn(async move {
                                collect_output(
                                    procs,
                                    proc_id,
                                    generation,
                                    u64::from(restart_attempt),
                                    stderr,
                                    LogStream::Stderr,
                                    None,
                                )
                                .await;
                            });
                        }

                        // Mark as running after brief startup
                        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
                        let mut procs = processes.write().await;
                        if let Some(proc) = procs.get_mut(&id).filter(|proc| {
                            proc.generation == generation
                                && proc.incarnation == u64::from(restart_attempt)
                        }) {
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
                        if let Some(proc) = procs.get_mut(&id).filter(|proc| {
                            proc.generation == generation
                                && matches!(
                                    proc.status,
                                    ProcessStatus::Restarting { attempt }
                                        if attempt == restart_attempt
                                )
                        }) {
                            proc.status = ProcessStatus::Crashed { exit_code };
                        }
                    }
                }
            } else if !matches!(
                proc.status,
                ProcessStatus::Stopped | ProcessStatus::Restarting { .. }
            ) {
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
        .sanitized_env()
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
