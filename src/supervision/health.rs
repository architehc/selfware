//! Health check system for monitoring component health

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
