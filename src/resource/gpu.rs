//! GPU resource management

use crate::config::GpuConfig;
use crate::errors::{ResourceError, SelfwareError};
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;
use tracing::{debug, info, warn};

/// GPU manager for monitoring and controlling GPU resources
pub struct GpuManager {
    config: GpuConfig,
    nvml: Option<nvml_wrapper::Nvml>,
    devices: Vec<GpuDevice>,
    throttled: AtomicU32,
}

/// GPU device information
#[derive(Debug, Clone)]
pub struct GpuDevice {
    pub index: u32,
    pub uuid: String,
    pub name: String,
    pub memory_total: u64,
    pub memory_allocated: Arc<AtomicU64>,
}

/// GPU usage statistics
#[derive(Debug, Clone, Default)]
pub struct GpuUsage {
    pub memory_used: u64,
    pub memory_total: u64,
    pub utilization: f32,
    pub temperature: u32,
    pub power_draw: f32,
}

/// Quantization level for model compression
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum QuantizationLevel {
    None, // Full precision (FP16/FP32)
    FP8,  // 8-bit floating point
    Int8, // 8-bit integer
    Int4, // 4-bit integer
}

impl GpuManager {
    /// Create a new GPU manager
    pub async fn new(config: &GpuConfig) -> Result<Self, SelfwareError> {
        let nvml = nvml_wrapper::Nvml::init().ok();
        let mut devices = Vec::new();

        if let Some(ref nvml) = nvml {
            match nvml.device_count() {
                Ok(count) => {
                    for i in 0..count {
                        match nvml.device_by_index(i) {
                            Ok(device) => {
                                let uuid = device.uuid().unwrap_or_default();
                                let name = device.name().unwrap_or_default();
                                let memory = device.memory_info().unwrap();

                                devices.push(GpuDevice {
                                    index: i,
                                    uuid,
                                    name: name.clone(),
                                    memory_total: memory.total,
                                    memory_allocated: Arc::new(AtomicU64::new(0)),
                                });

                                info!(
                                    index = i,
                                    name = %name,
                                    memory_gb = memory.total / 1_000_000_000,
                                    "GPU device found"
                                );
                            }
                            Err(e) => {
                                warn!(index = i, error = %e, "Failed to get GPU device info");
                            }
                        }
                    }
                }
                Err(e) => {
                    warn!(error = %e, "Failed to get GPU device count");
                }
            }
        } else {
            warn!("NVML not available, GPU monitoring disabled");
        }

        Ok(Self {
            config: config.clone(),
            nvml,
            devices,
            throttled: AtomicU32::new(0),
        })
    }

    /// Get current GPU usage
    pub async fn get_usage(&self) -> Result<GpuUsage, ResourceError> {
        let Some(ref nvml) = self.nvml else {
            return Ok(GpuUsage::default());
        };

        let mut total_usage = GpuUsage::default();

        for device in &self.devices {
            match nvml.device_by_index(device.index) {
                Ok(dev) => {
                    // Memory info
                    if let Ok(mem) = dev.memory_info() {
                        total_usage.memory_used += mem.used;
                        total_usage.memory_total += mem.total;
                    }

                    // Utilization
                    if let Ok(util) = dev.utilization_rates() {
                        total_usage.utilization = total_usage.utilization.max(util.gpu as f32);
                    }

                    // Temperature
                    if let Ok(temp) =
                        dev.temperature(nvml_wrapper::enum_wrappers::device::TemperatureSensor::Gpu)
                    {
                        total_usage.temperature = total_usage.temperature.max(temp);
                    }

                    // Power
                    if let Ok(power) = dev.power_usage() {
                        total_usage.power_draw += power as f32 / 1000.0;
                    }
                }
                Err(e) => {
                    debug!(index = device.index, error = %e, "Failed to get GPU stats");
                }
            }
        }

        Ok(total_usage)
    }

    /// Get available GPU memory
    pub async fn get_available_memory(&self) -> u64 {
        if let Ok(usage) = self.get_usage().await {
            usage.memory_total.saturating_sub(usage.memory_used)
        } else {
            0
        }
    }

    /// Monitor GPU continuously
    pub async fn monitor(&self) {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(
            self.config.monitor_interval_seconds,
        ));

        loop {
            interval.tick().await;

            if let Ok(usage) = self.get_usage().await {
                // Check temperature
                if usage.temperature > self.config.temperature_threshold {
                    warn!(temp = usage.temperature, "GPU temperature high");

                    if self.config.throttle_on_overheat {
                        self.throttle_compute(0.7).await;
                    }
                }

                // Check memory utilization
                let mem_util = if usage.memory_total > 0 {
                    usage.memory_used as f32 / usage.memory_total as f32
                } else {
                    0.0
                };

                if mem_util > self.config.memory_utilization_threshold {
                    warn!(utilization = mem_util, "GPU memory utilization high");
                }

                // Emit metrics
                // metrics::gauge!("gpu.memory.used_bytes", usage.memory_used as f64);
                // metrics::gauge!("gpu.utilization", usage.utilization as f64);
                // metrics::gauge!("gpu.temperature", usage.temperature as f64);
                // metrics::gauge!("gpu.power_draw", usage.power_draw as f64);
            }
        }
    }

    /// Throttle GPU compute
    pub async fn throttle_compute(&self, factor: f32) {
        let current = self.throttled.load(Ordering::Relaxed);
        let new = (current as f32 * factor) as u32;
        self.throttled.store(new, Ordering::Relaxed);

        warn!(throttle_factor = factor, "GPU compute throttled");

        // In a real implementation, this would adjust vLLM batch sizes,
        // reduce concurrent requests, etc.
    }

    /// Reduce batch size for inference
    pub async fn reduce_batch_size(&self) {
        warn!("Reducing GPU batch size");
        // This would communicate with the LLM engine to reduce batch size
    }

    /// Determine appropriate quantization level based on available memory
    pub async fn adjust_quantization(&self, required_memory: u64) -> QuantizationLevel {
        let available = self.get_available_memory().await;

        if available > required_memory * 2 {
            QuantizationLevel::None
        } else if available as f64 > required_memory as f64 * 1.5 {
            QuantizationLevel::FP8
        } else if available > required_memory {
            QuantizationLevel::Int8
        } else if available as f64 > required_memory as f64 * 0.6 {
            QuantizationLevel::Int4
        } else {
            // Not enough memory even with int4
            warn!("Insufficient GPU memory even with quantization");
            QuantizationLevel::Int4
        }
    }

    /// Allocate GPU memory for a model
    pub async fn allocate_memory(
        &self,
        device_index: u32,
        bytes: u64,
    ) -> Result<(), ResourceError> {
        if let Some(device) = self.devices.get(device_index as usize) {
            let current = device.memory_allocated.load(Ordering::Relaxed);
            let new_total = current + bytes;

            if new_total > device.memory_total {
                return Err(ResourceError::Gpu(format!(
                    "Cannot allocate {} bytes, only {} available",
                    bytes,
                    device.memory_total - current
                )));
            }

            device.memory_allocated.store(new_total, Ordering::Relaxed);
            debug!(
                device = device_index,
                allocated_bytes = bytes,
                "GPU memory allocated"
            );

            Ok(())
        } else {
            Err(ResourceError::Gpu(format!(
                "Invalid device index: {}",
                device_index
            )))
        }
    }

    /// Free GPU memory
    pub async fn free_memory(&self, device_index: u32, bytes: u64) {
        if let Some(device) = self.devices.get(device_index as usize) {
            let current = device.memory_allocated.load(Ordering::Relaxed);
            let new_total = current.saturating_sub(bytes);
            device.memory_allocated.store(new_total, Ordering::Relaxed);

            debug!(
                device = device_index,
                freed_bytes = bytes,
                "GPU memory freed"
            );
        }
    }

    /// Get list of GPU devices
    pub fn devices(&self) -> &[GpuDevice] {
        &self.devices
    }
}

#[cfg(test)]
mod tests {
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
        assert_eq!(device.memory_allocated.load(Ordering::Relaxed), 10_000_000_000);
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
}
