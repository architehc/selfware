//! Health check system for monitoring component health

use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tracing::{debug, error, warn};

/// Health check trait for components
#[async_trait::async_trait]
pub trait HealthCheck: Send + Sync {
    /// Get the name of this health check
    fn name(&self) -> &str;

    /// Perform the health check
    async fn check(&self) -> HealthStatus;
}

/// Health status
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HealthStatus {
    /// Component is healthy
    Healthy,
    /// Component is degraded but functional
    Degraded { reason: String },
    /// Component is unhealthy
    Unhealthy { reason: String, severity: Severity },
}

/// Severity levels for unhealthy status
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    /// Warning - can continue operating
    Warning,
    /// Critical - should take action
    Critical,
    /// Fatal - system should stop
    Fatal,
}

/// Health monitor for running periodic health checks
pub struct HealthMonitor {
    checks: Vec<Box<dyn HealthCheck + Send + Sync>>,
    interval: Duration,
    _failure_threshold: u32,
    results: Arc<RwLock<Vec<HealthCheckResult>>>,
}

/// Health check result
#[derive(Debug, Clone)]
pub struct HealthCheckResult {
    pub name: String,
    pub status: HealthStatus,
    pub checked_at: Instant,
    pub response_time: Duration,
}

/// Overall health status
#[derive(Debug, Clone)]
pub struct OverallHealth {
    pub status: OverallStatus,
    pub checks: Vec<HealthCheckResult>,
    pub checked_at: Instant,
}

/// Overall status
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverallStatus {
    Healthy,
    Degraded,
    Unhealthy,
}

impl HealthMonitor {
    /// Create a new health monitor
    pub fn new(interval: Duration, failure_threshold: u32) -> Self {
        Self {
            checks: Vec::new(),
            interval,
            _failure_threshold: failure_threshold,
            results: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Add a health check
    pub fn add_check(&mut self, check: Box<dyn HealthCheck + Send + Sync>) {
        self.checks.push(check);
    }

    /// Start the health monitor loop
    pub async fn start(&self) {
        let mut interval = tokio::time::interval(self.interval);

        loop {
            interval.tick().await;

            let mut results = Vec::new();

            for check in &self.checks {
                let start = Instant::now();
                let status = check.check().await;
                let response_time = start.elapsed();

                results.push(HealthCheckResult {
                    name: check.name().to_string(),
                    status,
                    checked_at: Instant::now(),
                    response_time,
                });
            }

            // Store results
            *self.results.write().await = results.clone();

            // Update the global health flag used by the HTTP health
            // endpoint so that Docker/Kubernetes probes reflect the real
            // status.  If any check is Unhealthy, the endpoint returns 503.
            let any_unhealthy = results
                .iter()
                .any(|r| matches!(r.status, HealthStatus::Unhealthy { .. }));
            set_global_healthy(!any_unhealthy);

            // Log any issues
            for result in &results {
                match &result.status {
                    HealthStatus::Healthy => {}
                    HealthStatus::Degraded { reason } => {
                        warn!(check = %result.name, reason = %reason, "Health check degraded");
                    }
                    HealthStatus::Unhealthy { reason, severity } => {
                        error!(check = %result.name, reason = %reason, severity = ?severity, "Health check failed");
                    }
                }
            }
        }
    }

    /// Get current health status
    pub async fn health(&self) -> OverallHealth {
        let results = self.results.read().await.clone();

        let status = if results
            .iter()
            .all(|r| matches!(r.status, HealthStatus::Healthy))
        {
            OverallStatus::Healthy
        } else if results
            .iter()
            .any(|r| matches!(r.status, HealthStatus::Unhealthy { .. }))
        {
            OverallStatus::Unhealthy
        } else {
            OverallStatus::Degraded
        };

        OverallHealth {
            status,
            checks: results,
            checked_at: Instant::now(),
        }
    }
}

/// Agent heartbeat health check
pub struct AgentHealthCheck {
    last_heartbeat: Arc<RwLock<Option<Instant>>>,
    heartbeat_timeout: Duration,
    name: String,
}

impl AgentHealthCheck {
    /// Create a new agent health check
    pub fn new(name: impl Into<String>, heartbeat_timeout: Duration) -> Self {
        Self {
            last_heartbeat: Arc::new(RwLock::new(None)),
            heartbeat_timeout,
            name: name.into(),
        }
    }

    /// Record a heartbeat
    pub async fn heartbeat(&self) {
        *self.last_heartbeat.write().await = Some(Instant::now());
        debug!("Heartbeat recorded");
    }
}

#[async_trait::async_trait]
impl HealthCheck for AgentHealthCheck {
    fn name(&self) -> &str {
        &self.name
    }

    async fn check(&self) -> HealthStatus {
        let last = *self.last_heartbeat.read().await;

        match last {
            None => HealthStatus::Unhealthy {
                reason: "No heartbeat received".to_string(),
                severity: Severity::Critical,
            },
            Some(instant) => {
                let elapsed = instant.elapsed();

                if elapsed > self.heartbeat_timeout * 2 {
                    HealthStatus::Unhealthy {
                        reason: format!("Heartbeat timeout: {:?}", elapsed),
                        severity: Severity::Critical,
                    }
                } else if elapsed > self.heartbeat_timeout {
                    HealthStatus::Degraded {
                        reason: format!("Slow heartbeat: {:?}", elapsed),
                    }
                } else {
                    HealthStatus::Healthy
                }
            }
        }
    }
}

/// GPU health check
pub struct GpuHealthCheck {
    nvml: Option<nvml_wrapper::Nvml>,
}

impl Default for GpuHealthCheck {
    fn default() -> Self {
        Self::new()
    }
}

impl GpuHealthCheck {
    /// Create a new GPU health check
    pub fn new() -> Self {
        Self {
            nvml: nvml_wrapper::Nvml::init().ok(),
        }
    }
}

#[async_trait::async_trait]
impl HealthCheck for GpuHealthCheck {
    fn name(&self) -> &str {
        "gpu"
    }

    async fn check(&self) -> HealthStatus {
        let Some(nvml) = self.nvml.as_ref() else {
            return HealthStatus::Unhealthy {
                reason: "NVML not available".to_string(),
                severity: Severity::Critical,
            };
        };

        match nvml.device_by_index(0) {
            Ok(device) => {
                // Check temperature
                let temp =
                    device.temperature(nvml_wrapper::enum_wrappers::device::TemperatureSensor::Gpu);
                match temp {
                    Ok(t) if t > 90 => HealthStatus::Unhealthy {
                        reason: format!("GPU overheating: {}°C", t),
                        severity: Severity::Critical,
                    },
                    Ok(t) if t > 80 => HealthStatus::Degraded {
                        reason: format!("GPU temperature high: {}°C", t),
                    },
                    Err(e) => HealthStatus::Degraded {
                        reason: format!("Failed to read GPU temperature: {}", e),
                    },
                    _ => HealthStatus::Healthy,
                }
            }
            Err(e) => HealthStatus::Unhealthy {
                reason: format!("Failed to get GPU device: {}", e),
                severity: Severity::Critical,
            },
        }
    }
}

/// Memory health check
pub struct MemoryHealthCheck {
    warning_threshold: f32,
    critical_threshold: f32,
}

impl MemoryHealthCheck {
    /// Create a new memory health check
    pub fn new(warning_threshold: f32, critical_threshold: f32) -> Self {
        Self {
            warning_threshold,
            critical_threshold,
        }
    }
}

#[async_trait::async_trait]
impl HealthCheck for MemoryHealthCheck {
    fn name(&self) -> &str {
        "memory"
    }

    async fn check(&self) -> HealthStatus {
        use sysinfo::System;

        let mut system = System::new_all();
        system.refresh_all();

        let total = system.total_memory() as f32;
        let used = system.used_memory() as f32;
        let usage = used / total;

        if usage > self.critical_threshold {
            HealthStatus::Unhealthy {
                reason: format!("Memory critical: {:.1}% used", usage * 100.0),
                severity: Severity::Critical,
            }
        } else if usage > self.warning_threshold {
            HealthStatus::Degraded {
                reason: format!("Memory high: {:.1}% used", usage * 100.0),
            }
        } else {
            HealthStatus::Healthy
        }
    }
}

/// Disk health check
pub struct DiskHealthCheck {
    path: std::path::PathBuf,
    warning_threshold: f32,
    critical_threshold: f32,
}

impl DiskHealthCheck {
    /// Create a new disk health check
    pub fn new(
        path: impl Into<std::path::PathBuf>,
        warning_threshold: f32,
        critical_threshold: f32,
    ) -> Self {
        Self {
            path: path.into(),
            warning_threshold,
            critical_threshold,
        }
    }
}

#[async_trait::async_trait]
impl HealthCheck for DiskHealthCheck {
    fn name(&self) -> &str {
        "disk"
    }

    async fn check(&self) -> HealthStatus {
        use sysinfo::Disks;

        let disks = Disks::new_with_refreshed_list();

        for disk in disks.list() {
            if disk.mount_point() == self.path {
                let total = disk.total_space() as f32;
                let available = disk.available_space() as f32;
                let usage = 1.0 - (available / total);

                if usage > self.critical_threshold {
                    return HealthStatus::Unhealthy {
                        reason: format!("Disk critical: {:.1}% full", usage * 100.0),
                        severity: Severity::Critical,
                    };
                } else if usage > self.warning_threshold {
                    return HealthStatus::Degraded {
                        reason: format!("Disk high: {:.1}% full", usage * 100.0),
                    };
                } else {
                    return HealthStatus::Healthy;
                }
            }
        }

        HealthStatus::Degraded {
            reason: format!("Disk {} not found", self.path.display()),
        }
    }
}

/// Global health flag that can be updated by the supervision system.
/// Defaults to `true` (healthy) so that bare-bones deployments without
/// supervision still report liveness.
static GLOBAL_HEALTHY: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(true);

/// Update the global health flag used by the HTTP health endpoint.
pub fn set_global_healthy(healthy: bool) {
    GLOBAL_HEALTHY.store(healthy, std::sync::atomic::Ordering::Relaxed);
}

/// Epoch-milliseconds of the last agent-loop heartbeat. 0 means "no heartbeat
/// has ever been recorded" (bare-bones deployments), which is treated as
/// healthy so liveness isn't failed for callers that don't ping.
static LAST_HEARTBEAT_MS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Record a liveness heartbeat. Call this once per agent-loop iteration so the
/// health endpoint reflects real progress: a process whose loop is turning stays
/// healthy; one that hangs stops pinging and goes stale, so `systemd
/// WatchdogSec=` (or a k8s liveness probe) can actually restart it.
pub fn record_heartbeat() {
    // Never store 0 — that's the "never pinged" sentinel.
    LAST_HEARTBEAT_MS.store(now_ms().max(1), std::sync::atomic::Ordering::Relaxed);
}

/// Staleness threshold (seconds) before a heartbeat is considered dead. Default
/// is generous (a single step may legitimately take up to the step timeout);
/// operators can tune it via `SELFWARE_HEARTBEAT_STALE_SECS`.
fn heartbeat_stale_secs() -> u64 {
    std::env::var("SELFWARE_HEARTBEAT_STALE_SECS")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .filter(|&n| n > 0)
        .unwrap_or(900)
}

/// True unless a heartbeat was being recorded and has since gone stale.
fn heartbeat_healthy() -> bool {
    let last = LAST_HEARTBEAT_MS.load(std::sync::atomic::Ordering::Relaxed);
    if last == 0 {
        return true; // never pinged — don't fail liveness for non-supervised runs
    }
    now_ms().saturating_sub(last) <= heartbeat_stale_secs() * 1000
}

#[cfg(test)]
fn set_last_heartbeat_ms_for_test(ms: u64) {
    LAST_HEARTBEAT_MS.store(ms, std::sync::atomic::Ordering::Relaxed);
}

/// Start a minimal HTTP health endpoint on the given port.
///
/// Returns **200** with body `healthy\n` when the supervised component is
/// healthy, or **503** with body `unhealthy\n` when it is not.  This is
/// designed for Docker HEALTHCHECK and Kubernetes liveness probes.
pub async fn start_health_endpoint(port: u16) -> anyhow::Result<()> {
    use tokio::io::AsyncWriteExt;
    use tokio::net::TcpListener;

    let addr = format!("127.0.0.1:{}", port);
    let listener = TcpListener::bind(&addr).await?;
    tracing::info!("Health endpoint listening on {}", addr);

    loop {
        if let Ok((mut stream, _)) = listener.accept().await {
            let healthy =
                GLOBAL_HEALTHY.load(std::sync::atomic::Ordering::Relaxed) && heartbeat_healthy();
            let response = if healthy {
                "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: 9\r\n\r\nhealthy\n"
            } else {
                "HTTP/1.1 503 Service Unavailable\r\nContent-Type: text/plain\r\nContent-Length: 11\r\n\r\nunhealthy\n"
            };
            let _ = stream.write_all(response.as_bytes()).await;
            let _ = stream.shutdown().await;
        }
    }
}

/// Start health endpoint if SELFWARE_HEALTH_PORT env var is set.
/// Spawns the server as a background tokio task.
pub fn maybe_start_health_endpoint() {
    if let Ok(port_str) = std::env::var("SELFWARE_HEALTH_PORT") {
        if let Ok(port) = port_str.parse::<u16>() {
            tokio::spawn(async move {
                if let Err(e) = start_health_endpoint(port).await {
                    tracing::error!("Health endpoint failed: {}", e);
                }
            });
        }
    }
}

#[cfg(test)]
#[path = "../../tests/unit/supervision/health/health_test.rs"]
mod heartbeat_tests;

#[cfg(test)]
#[path = "../../tests/unit/supervision/health/health_main_test.rs"]
mod tests;
