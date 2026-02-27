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
    failure_threshold: u32,
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
            failure_threshold,
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
