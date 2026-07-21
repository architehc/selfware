use super::*;

/// Helper to create a GpuManager with synthetic devices (no NVML required).
fn make_test_gpu_manager(devices: Vec<GpuDevice>) -> GpuManager {
    GpuManager {
        config: GpuConfig::default(),
        nvml: None,
        devices,
        throttled: AtomicU32::new(100),
    }
}

fn make_test_device(index: u32, memory_total: u64) -> GpuDevice {
    GpuDevice {
        index,
        uuid: format!("GPU-TEST-{}", index),
        name: format!("Test GPU {}", index),
        memory_total,
        memory_allocated: Arc::new(AtomicU64::new(0)),
    }
}

// ---- QuantizationLevel tests ----

#[test]
fn test_quantization_level_ordering() {
    assert!(QuantizationLevel::None < QuantizationLevel::FP8);
    assert!(QuantizationLevel::FP8 < QuantizationLevel::Int8);
    assert!(QuantizationLevel::Int8 < QuantizationLevel::Int4);
}

#[test]
fn test_quantization_level_equality() {
    assert_eq!(QuantizationLevel::None, QuantizationLevel::None);
    assert_eq!(QuantizationLevel::Int4, QuantizationLevel::Int4);
    assert_ne!(QuantizationLevel::None, QuantizationLevel::Int8);
}

#[test]
fn test_quantization_level_copy() {
    let q = QuantizationLevel::FP8;
    let q2 = q;
    assert_eq!(q, q2);
}

// ---- GpuUsage tests ----

#[test]
fn test_gpu_usage_default() {
    let usage = GpuUsage::default();
    assert_eq!(usage.memory_used, 0);
    assert_eq!(usage.memory_total, 0);
    assert_eq!(usage.utilization, 0.0);
    assert_eq!(usage.temperature, 0);
    assert_eq!(usage.power_draw, 0.0);
}

#[test]
fn test_gpu_usage_clone() {
    let usage = GpuUsage {
        memory_used: 8_000_000_000,
        memory_total: 24_000_000_000,
        utilization: 75.0,
        temperature: 68,
        power_draw: 250.0,
    };
    let cloned = usage.clone();
    assert_eq!(cloned.memory_used, 8_000_000_000);
    assert_eq!(cloned.temperature, 68);
}

// ---- GpuDevice memory tracking tests ----

#[test]
fn test_gpu_device_memory_allocated_starts_at_zero() {
    let device = make_test_device(0, 24_000_000_000);
    assert_eq!(device.memory_allocated.load(Ordering::Relaxed), 0);
}

#[test]
fn test_gpu_device_memory_tracking() {
    let device = make_test_device(0, 24_000_000_000);
    device
        .memory_allocated
        .store(10_000_000_000, Ordering::Relaxed);
    assert_eq!(
        device.memory_allocated.load(Ordering::Relaxed),
        10_000_000_000
    );
}

// ---- GpuManager allocate/free tests ----

#[tokio::test]
async fn test_allocate_memory_success() {
    let device = make_test_device(0, 24_000_000_000);
    let gm = make_test_gpu_manager(vec![device]);

    let result = gm.allocate_memory(0, 10_000_000_000).await;
    assert!(result.is_ok());
    assert_eq!(
        gm.devices[0].memory_allocated.load(Ordering::Relaxed),
        10_000_000_000
    );
}

#[tokio::test]
async fn test_allocate_memory_exceeds_total() {
    let device = make_test_device(0, 10_000_000_000);
    let gm = make_test_gpu_manager(vec![device]);

    let result = gm.allocate_memory(0, 15_000_000_000).await;
    assert!(result.is_err());
    match result {
        Err(ResourceError::Gpu(msg)) => {
            assert!(msg.contains("Cannot allocate"));
        }
        _ => panic!("Expected ResourceError::Gpu"),
    }
}

#[tokio::test]
async fn test_allocate_memory_invalid_device() {
    let gm = make_test_gpu_manager(vec![]);
    let result = gm.allocate_memory(0, 100).await;
    assert!(result.is_err());
    match result {
        Err(ResourceError::Gpu(msg)) => {
            assert!(msg.contains("Invalid device index"));
        }
        _ => panic!("Expected ResourceError::Gpu for invalid device"),
    }
}

#[tokio::test]
async fn test_allocate_then_free_memory() {
    let device = make_test_device(0, 24_000_000_000);
    let gm = make_test_gpu_manager(vec![device]);

    gm.allocate_memory(0, 8_000_000_000).await.unwrap();
    assert_eq!(
        gm.devices[0].memory_allocated.load(Ordering::Relaxed),
        8_000_000_000
    );

    gm.free_memory(0, 3_000_000_000).await;
    assert_eq!(
        gm.devices[0].memory_allocated.load(Ordering::Relaxed),
        5_000_000_000
    );
}

#[tokio::test]
async fn test_free_memory_saturates_at_zero() {
    let device = make_test_device(0, 24_000_000_000);
    let gm = make_test_gpu_manager(vec![device]);

    gm.allocate_memory(0, 1_000).await.unwrap();
    gm.free_memory(0, 999_999).await;
    // Should not underflow due to saturating_sub
    assert_eq!(gm.devices[0].memory_allocated.load(Ordering::Relaxed), 0);
}

#[tokio::test]
async fn test_free_memory_invalid_device_does_not_panic() {
    let gm = make_test_gpu_manager(vec![]);
    // Should just be a no-op, not panic
    gm.free_memory(5, 1000).await;
}

#[tokio::test]
async fn test_multiple_allocations_accumulate() {
    let device = make_test_device(0, 24_000_000_000);
    let gm = make_test_gpu_manager(vec![device]);

    gm.allocate_memory(0, 5_000_000_000).await.unwrap();
    gm.allocate_memory(0, 3_000_000_000).await.unwrap();
    assert_eq!(
        gm.devices[0].memory_allocated.load(Ordering::Relaxed),
        8_000_000_000
    );
}

// ---- GpuManager throttle tests ----

#[tokio::test]
async fn test_throttle_compute() {
    let gm = make_test_gpu_manager(vec![]);
    // Initial throttle value is 100
    gm.throttle_compute(0.5).await;
    assert_eq!(gm.throttled.load(Ordering::Relaxed), 50);
}

// ---- GpuManager devices accessor ----

#[test]
fn test_devices_accessor_empty() {
    let gm = make_test_gpu_manager(vec![]);
    assert!(gm.devices().is_empty());
}

#[test]
fn test_devices_accessor_returns_all() {
    let devices = vec![
        make_test_device(0, 24_000_000_000),
        make_test_device(1, 16_000_000_000),
    ];
    let gm = make_test_gpu_manager(devices);
    assert_eq!(gm.devices().len(), 2);
    assert_eq!(gm.devices()[0].name, "Test GPU 0");
    assert_eq!(gm.devices()[1].memory_total, 16_000_000_000);
}
