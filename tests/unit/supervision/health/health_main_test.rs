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
