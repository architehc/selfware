use super::*;

// ---- ResourceManager construction/wiring tests ----
//
// Prior to this fix, nothing in the codebase ever constructed a
// ResourceManager or ticked its monitor loop outside these tests
// themselves, so GPU/memory/disk threshold protection never actually ran.

#[tokio::test]
async fn test_resource_manager_new_succeeds_without_gpu_present() {
    // Nvml::init() gracefully returns None when no GPU/driver is
    // present -- construction must not fail on a GPU-less machine (e.g.
    // this CI/test environment).
    let config = ResourcesConfig::default();
    let manager = ResourceManager::new(&config).await;
    assert!(
        manager.is_ok(),
        "ResourceManager::new should succeed without a GPU present: {:?}",
        manager.err()
    );
}

#[tokio::test]
async fn test_resource_manager_update_usage_reports_real_memory() {
    let config = ResourcesConfig::default();
    let manager = ResourceManager::new(&config).await.unwrap();
    manager.update_usage().await;
    let usage = manager.get_usage().await;
    assert!(
        usage.memory_total_bytes > 0,
        "expected real system memory stats after update_usage, got {:?}",
        usage
    );
}

#[tokio::test]
async fn test_resource_manager_pressure_computation_does_not_panic() {
    let config = ResourcesConfig::default();
    let manager = ResourceManager::new(&config).await.unwrap();
    manager.update_usage().await;
    // Whatever this machine's actual usage is, this must return some
    // valid pressure variant without panicking (e.g. divide-by-zero on
    // a machine with gpu_memory_total_bytes == 0 is guarded already).
    let _pressure = manager.get_resource_pressure().await;
}

#[tokio::test]
async fn test_resource_manager_shared_pressure_handle_starts_at_none() {
    let config = ResourcesConfig::default();
    let manager = ResourceManager::new(&config).await.unwrap();
    let handle = manager.shared_pressure();
    assert_eq!(*handle.read().unwrap(), ResourcePressure::None);
}

// ---- monitor_loop shutdown tests ----

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_monitor_loop_exits_when_sender_dropped_without_sending() {
    // The normal process-exit path drops the shutdown handle WITHOUT
    // ever sending `true`. Previously the loop only checked the *value*,
    // so `changed()` re-resolved instantly forever and the monitor
    // busy-spun full System::new_all() /proc scans (~12s per command
    // exit) until tokio coop forced a yield. The loop must instead treat
    // sender-drop as shutdown and return promptly.
    let config = ResourcesConfig::default();
    let manager = ResourceManager::new(&config).await.unwrap();
    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
    drop(shutdown_tx); // drop WITHOUT sending true — the exact exit path

    tokio::time::timeout(Duration::from_secs(2), manager.monitor_loop(shutdown_rx))
        .await
        .expect("monitor_loop must exit promptly when the shutdown sender is dropped");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_monitor_loop_exits_on_explicit_shutdown_signal() {
    let config = ResourcesConfig::default();
    let manager = ResourceManager::new(&config).await.unwrap();
    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
    shutdown_tx.send(true).unwrap();

    tokio::time::timeout(Duration::from_secs(2), manager.monitor_loop(shutdown_rx))
        .await
        .expect("monitor_loop must exit promptly on an explicit shutdown signal");
}

// ---- ResourcePressure tests ----

#[test]
fn test_resource_pressure_is_critical_only_for_critical() {
    assert!(ResourcePressure::Critical.is_critical());
    assert!(!ResourcePressure::High.is_critical());
    assert!(!ResourcePressure::Medium.is_critical());
    assert!(!ResourcePressure::Low.is_critical());
    assert!(!ResourcePressure::None.is_critical());
}

#[test]
fn test_resource_pressure_requires_action() {
    assert!(ResourcePressure::Critical.requires_action());
    assert!(ResourcePressure::High.requires_action());
    assert!(ResourcePressure::Medium.requires_action());
    assert!(!ResourcePressure::Low.requires_action());
    assert!(!ResourcePressure::None.requires_action());
}

#[test]
fn test_resource_pressure_equality() {
    assert_eq!(ResourcePressure::None, ResourcePressure::None);
    assert_eq!(ResourcePressure::Critical, ResourcePressure::Critical);
    assert_ne!(ResourcePressure::Low, ResourcePressure::High);
}

#[test]
fn test_resource_pressure_clone() {
    let p = ResourcePressure::High;
    let p2 = p;
    assert_eq!(p, p2);
}

// ---- ResourceUsage tests ----

#[test]
fn test_resource_usage_default() {
    let usage = ResourceUsage::default();
    assert_eq!(usage.cpu_percent, 0.0);
    assert_eq!(usage.memory_used_bytes, 0);
    assert_eq!(usage.memory_total_bytes, 0);
    assert_eq!(usage.gpu_memory_used_bytes, 0);
    assert_eq!(usage.gpu_memory_total_bytes, 0);
    assert_eq!(usage.gpu_utilization, 0.0);
    assert_eq!(usage.gpu_temperature, 0);
    assert_eq!(usage.disk_used_bytes, 0);
    assert_eq!(usage.disk_total_bytes, 0);
}

#[test]
fn test_resource_usage_clone() {
    let usage = ResourceUsage {
        cpu_percent: 55.0,
        memory_used_bytes: 8_000_000_000,
        memory_total_bytes: 16_000_000_000,
        gpu_memory_used_bytes: 4_000_000_000,
        gpu_memory_total_bytes: 24_000_000_000,
        gpu_utilization: 80.0,
        gpu_temperature: 72,
        disk_used_bytes: 500_000_000_000,
        disk_total_bytes: 1_000_000_000_000,
    };
    let cloned = usage.clone();
    assert_eq!(cloned.cpu_percent, 55.0);
    assert_eq!(cloned.memory_used_bytes, 8_000_000_000);
    assert_eq!(cloned.gpu_temperature, 72);
}

// ---- ResourceRequest tests ----

#[test]
fn test_resource_request_creation() {
    let req = ResourceRequest {
        gpu_memory_bytes: 10_000_000_000,
        system_memory_bytes: 4_000_000_000,
        disk_bytes: 1_000_000_000,
        duration_estimate: Duration::from_secs(300),
    };
    assert_eq!(req.gpu_memory_bytes, 10_000_000_000);
    assert_eq!(req.system_memory_bytes, 4_000_000_000);
    assert_eq!(req.disk_bytes, 1_000_000_000);
    assert_eq!(req.duration_estimate, Duration::from_secs(300));
}

#[test]
fn test_resource_request_clone() {
    let req = ResourceRequest {
        gpu_memory_bytes: 5_000_000_000,
        system_memory_bytes: 2_000_000_000,
        disk_bytes: 500_000_000,
        duration_estimate: Duration::from_secs(60),
    };
    let cloned = req.clone();
    assert_eq!(cloned.gpu_memory_bytes, req.gpu_memory_bytes);
    assert_eq!(cloned.duration_estimate, req.duration_estimate);
}

// ---- ResourceReservation tests ----

#[test]
fn test_resource_reservation_creation_and_release() {
    let req = ResourceRequest {
        gpu_memory_bytes: 1_000_000,
        system_memory_bytes: 500_000,
        disk_bytes: 100_000,
        duration_estimate: Duration::from_secs(10),
    };
    let reservation = ResourceReservation {
        request: req,
        reserved_at: std::time::Instant::now(),
    };
    assert_eq!(reservation.request.gpu_memory_bytes, 1_000_000);
    // release consumes self; just verify it doesn't panic
    reservation.release();
}

#[test]
fn test_resource_reservation_tracks_time() {
    let before = std::time::Instant::now();
    let req = ResourceRequest {
        gpu_memory_bytes: 0,
        system_memory_bytes: 0,
        disk_bytes: 0,
        duration_estimate: Duration::from_secs(0),
    };
    let reservation = ResourceReservation {
        request: req,
        reserved_at: std::time::Instant::now(),
    };
    let after = std::time::Instant::now();
    assert!(reservation.reserved_at >= before);
    assert!(reservation.reserved_at <= after);
}

#[test]
fn test_resource_reservation_clone() {
    let req = ResourceRequest {
        gpu_memory_bytes: 42,
        system_memory_bytes: 99,
        disk_bytes: 7,
        duration_estimate: Duration::from_millis(500),
    };
    let reservation = ResourceReservation {
        request: req,
        reserved_at: std::time::Instant::now(),
    };
    let cloned = reservation.clone();
    assert_eq!(
        cloned.request.gpu_memory_bytes,
        reservation.request.gpu_memory_bytes
    );
}
