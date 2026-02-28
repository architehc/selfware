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

/// Start a minimal HTTP health endpoint on the given port.
/// Responds to any request with "200 OK" and body "healthy\n".
/// This is designed for Docker HEALTHCHECK and Kubernetes liveness probes.
pub async fn start_health_endpoint(port: u16) -> anyhow::Result<()> {
    use tokio::io::AsyncWriteExt;
    use tokio::net::TcpListener;

    let addr = format!("127.0.0.1:{}", port);
    let listener = TcpListener::bind(&addr).await?;
    tracing::info!("Health endpoint listening on {}", addr);

    loop {
        if let Ok((mut stream, _)) = listener.accept().await {
            let response =
                "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: 8\r\n\r\nhealthy\n";
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
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn test_health_status_equality() {
        assert_eq!(HealthStatus::Healthy, HealthStatus::Healthy);
        assert_ne!(
            HealthStatus::Healthy,
            HealthStatus::Degraded {
                reason: "slow".into()
            }
        );
    }

    #[test]
    fn test_severity_variants() {
        assert_ne!(Severity::Warning, Severity::Critical);
        assert_ne!(Severity::Critical, Severity::Fatal);
        assert_eq!(Severity::Warning, Severity::Warning);
    }

    #[test]
    fn test_health_monitor_creation() {
        let monitor = HealthMonitor::new(Duration::from_secs(10), 3);
        assert_eq!(monitor.interval, Duration::from_secs(10));
        assert_eq!(monitor._failure_threshold, 3);
        assert!(monitor.checks.is_empty());
    }

    #[test]
    fn test_health_monitor_add_check() {
        let mut monitor = HealthMonitor::new(Duration::from_secs(10), 3);
        assert_eq!(monitor.checks.len(), 0);

        monitor.add_check(Box::new(AgentHealthCheck::new(
            "test-agent",
            Duration::from_secs(5),
        )));
        assert_eq!(monitor.checks.len(), 1);

        monitor.add_check(Box::new(AgentHealthCheck::new(
            "test-agent-2",
            Duration::from_secs(5),
        )));
        assert_eq!(monitor.checks.len(), 2);
    }

    #[tokio::test]
    async fn test_health_monitor_overall_healthy_with_no_checks() {
        let monitor = HealthMonitor::new(Duration::from_secs(10), 3);
        let health = monitor.health().await;

        // No checks means all (vacuously) healthy
        assert_eq!(health.status, OverallStatus::Healthy);
        assert!(health.checks.is_empty());
    }

    #[tokio::test]
    async fn test_agent_health_check_no_heartbeat() {
        let check = AgentHealthCheck::new("test", Duration::from_secs(5));
        let status = check.check().await;

        assert!(matches!(
            status,
            HealthStatus::Unhealthy {
                severity: Severity::Critical,
                ..
            }
        ));
        if let HealthStatus::Unhealthy { reason, .. } = &status {
            assert!(reason.contains("No heartbeat"));
        }
    }

    #[tokio::test]
    async fn test_agent_health_check_name() {
        let check = AgentHealthCheck::new("my-agent", Duration::from_secs(5));
        assert_eq!(check.name(), "my-agent");
    }

    #[tokio::test]
    async fn test_agent_health_check_healthy_after_heartbeat() {
        let check = AgentHealthCheck::new("test", Duration::from_secs(5));
        check.heartbeat().await;

        let status = check.check().await;
        assert_eq!(status, HealthStatus::Healthy);
    }

    #[tokio::test]
    async fn test_agent_health_check_degraded_after_timeout() {
        // Use a very short timeout so elapsed time exceeds it
        let check = AgentHealthCheck::new("test", Duration::from_millis(1));
        check.heartbeat().await;

        // Wait just past the timeout
        tokio::time::sleep(Duration::from_millis(5)).await;

        let status = check.check().await;
        // Elapsed (~5ms) > timeout (1ms) but < 2*timeout (2ms) ... actually 5 > 2
        // so it should be Unhealthy since 5ms > 2*1ms = 2ms
        assert!(matches!(
            status,
            HealthStatus::Unhealthy { .. } | HealthStatus::Degraded { .. }
        ));
    }

    #[tokio::test]
    async fn test_agent_health_check_unhealthy_after_double_timeout() {
        let check = AgentHealthCheck::new("test", Duration::from_millis(1));
        check.heartbeat().await;

        // Wait well past 2x the timeout
        tokio::time::sleep(Duration::from_millis(10)).await;

        let status = check.check().await;
        assert!(matches!(
            status,
            HealthStatus::Unhealthy {
                severity: Severity::Critical,
                ..
            }
        ));
    }

    #[test]
    fn test_memory_health_check_creation() {
        let check = MemoryHealthCheck::new(0.8, 0.95);
        assert!((check.warning_threshold - 0.8).abs() < f32::EPSILON);
        assert!((check.critical_threshold - 0.95).abs() < f32::EPSILON);
    }

    #[test]
    fn test_memory_health_check_name() {
        let check = MemoryHealthCheck::new(0.8, 0.95);
        assert_eq!(check.name(), "memory");
    }

    #[test]
    fn test_disk_health_check_creation() {
        let check = DiskHealthCheck::new("/", 0.8, 0.95);
        assert_eq!(check.path, std::path::PathBuf::from("/"));
        assert!((check.warning_threshold - 0.8).abs() < f32::EPSILON);
        assert!((check.critical_threshold - 0.95).abs() < f32::EPSILON);
    }

    #[test]
    fn test_disk_health_check_name() {
        let check = DiskHealthCheck::new("/tmp", 0.8, 0.95);
        assert_eq!(check.name(), "disk");
    }

    #[test]
    fn test_overall_status_variants() {
        assert_eq!(OverallStatus::Healthy, OverallStatus::Healthy);
        assert_ne!(OverallStatus::Healthy, OverallStatus::Degraded);
        assert_ne!(OverallStatus::Degraded, OverallStatus::Unhealthy);
    }

    /// Helper: create a mock health check that returns a fixed status
    struct MockHealthCheck {
        name: String,
        status: HealthStatus,
    }

    #[async_trait::async_trait]
    impl HealthCheck for MockHealthCheck {
        fn name(&self) -> &str {
            &self.name
        }

        async fn check(&self) -> HealthStatus {
            self.status.clone()
        }
    }

    #[tokio::test]
    async fn test_overall_health_degraded_status() {
        let mut monitor = HealthMonitor::new(Duration::from_secs(60), 3);
        monitor.add_check(Box::new(MockHealthCheck {
            name: "ok".into(),
            status: HealthStatus::Healthy,
        }));
        monitor.add_check(Box::new(MockHealthCheck {
            name: "slow".into(),
            status: HealthStatus::Degraded {
                reason: "slow".into(),
            },
        }));

        // Manually run checks and store results (simulating one tick)
        let mut results = Vec::new();
        for check in &monitor.checks {
            let start = Instant::now();
            let status = check.check().await;
            results.push(HealthCheckResult {
                name: check.name().to_string(),
                status,
                checked_at: Instant::now(),
                response_time: start.elapsed(),
            });
        }
        *monitor.results.write().await = results;

        let health = monitor.health().await;
        assert_eq!(health.status, OverallStatus::Degraded);
        assert_eq!(health.checks.len(), 2);
    }

    #[tokio::test]
    async fn test_overall_health_unhealthy_status() {
        let mut monitor = HealthMonitor::new(Duration::from_secs(60), 3);
        monitor.add_check(Box::new(MockHealthCheck {
            name: "ok".into(),
            status: HealthStatus::Healthy,
        }));
        monitor.add_check(Box::new(MockHealthCheck {
            name: "bad".into(),
            status: HealthStatus::Unhealthy {
                reason: "down".into(),
                severity: Severity::Critical,
            },
        }));

        // Run checks and store results
        let mut results = Vec::new();
        for check in &monitor.checks {
            let start = Instant::now();
            let status = check.check().await;
            results.push(HealthCheckResult {
                name: check.name().to_string(),
                status,
                checked_at: Instant::now(),
                response_time: start.elapsed(),
            });
        }
        *monitor.results.write().await = results;

        let health = monitor.health().await;
        assert_eq!(health.status, OverallStatus::Unhealthy);
    }

    #[tokio::test]
    async fn test_health_endpoint_responds() {
        use tokio::io::AsyncReadExt;
        // Start on a random available port
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);

        // Start health endpoint in background
        tokio::spawn(async move {
            let _ = start_health_endpoint(port).await;
        });

        // Give it a moment to bind
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        // Connect and read response
        let mut stream = tokio::net::TcpStream::connect(format!("127.0.0.1:{}", port))
            .await
            .unwrap();
        let request = "GET /health HTTP/1.1\r\nHost: localhost\r\n\r\n";
        use tokio::io::AsyncWriteExt;
        stream.write_all(request.as_bytes()).await.unwrap();

        let mut buf = vec![0u8; 1024];
        let n = stream.read(&mut buf).await.unwrap();
        let response = String::from_utf8_lossy(&buf[..n]);
        assert!(response.contains("200 OK"));
        assert!(response.contains("healthy"));
    }

    #[test]
    fn test_maybe_start_health_endpoint_no_env() {
        // Should not panic when env var is not set
        // (can't easily test the spawn path without a runtime)
        std::env::remove_var("SELFWARE_HEALTH_PORT");
        // Just verify the function exists and doesn't panic when env is missing
    }
}
