//! Resource management for CPU, GPU, memory, and disk

use crate::config::ResourcesConfig;
use crate::errors::{ResourceError, SelfwareError};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

pub mod disk;
pub mod gpu;
pub mod memory;
pub mod quotas;

pub use disk::DiskManager;
pub use gpu::GpuManager;
pub use memory::MemoryManager;
pub use quotas::AdaptiveQuotas;

/// Resource manager for coordinating all resource types
pub struct ResourceManager {
    config: ResourcesConfig,
    gpu: Arc<GpuManager>,
    memory: Arc<MemoryManager>,
    disk: Arc<DiskManager>,
    quotas: Arc<RwLock<AdaptiveQuotas>>,
    usage: Arc<RwLock<ResourceUsage>>,
}

/// Current resource usage
#[derive(Debug, Clone, Default)]
pub struct ResourceUsage {
    pub cpu_percent: f32,
    pub memory_used_bytes: u64,
    pub memory_total_bytes: u64,
    pub gpu_memory_used_bytes: u64,
    pub gpu_memory_total_bytes: u64,
    pub gpu_utilization: f32,
    pub gpu_temperature: u32,
    pub disk_used_bytes: u64,
    pub disk_total_bytes: u64,
}

/// Resource pressure levels
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResourcePressure {
    None,
    Low,
    Medium,
    High,
    Critical,
}

impl ResourcePressure {
    /// Check if pressure is critical
    pub fn is_critical(&self) -> bool {
        matches!(self, Self::Critical)
    }

    /// Check if pressure requires action
    pub fn requires_action(&self) -> bool {
        matches!(self, Self::Medium | Self::High | Self::Critical)
    }
}

impl ResourceManager {
    /// Create a new resource manager
    pub async fn new(config: &ResourcesConfig) -> Result<Self, SelfwareError> {
        let gpu = Arc::new(GpuManager::new(&config.gpu).await?);
        let memory = Arc::new(MemoryManager::new(&config.memory).await?);
        let disk = Arc::new(DiskManager::new(&config.disk).await?);

        let quotas = Arc::new(RwLock::new(AdaptiveQuotas::new(config.quotas.clone())));
        let usage = Arc::new(RwLock::new(ResourceUsage::default()));

        Ok(Self {
            config: config.clone(),
            gpu,
            memory,
            disk,
            quotas,
            usage,
        })
    }

    /// Start resource monitoring loop
    pub async fn monitor_loop(&self) {
        let mut interval = tokio::time::interval(Duration::from_secs(5));

        loop {
            interval.tick().await;

            // Update resource usage
            self.update_usage().await;

            // Check resource pressure
            let pressure = self.get_resource_pressure().await;

            if pressure.requires_action() {
                warn!(pressure = ?pressure, "Resource pressure detected");
                self.handle_pressure(pressure).await;
            }

            // Update adaptive quotas
            {
                let quotas = self.quotas.write().await;
                quotas.adjust_for_pressure(pressure).await;
            }
        }
    }

    /// Update current resource usage
    async fn update_usage(&self) {
        let mut usage = self.usage.write().await;

        // Get GPU usage
        if let Ok(gpu_usage) = self.gpu.get_usage().await {
            usage.gpu_memory_used_bytes = gpu_usage.memory_used;
            usage.gpu_memory_total_bytes = gpu_usage.memory_total;
            usage.gpu_utilization = gpu_usage.utilization;
            usage.gpu_temperature = gpu_usage.temperature;
        }

        // Get memory usage
        if let Ok(mem_usage) = self.memory.get_usage().await {
            usage.memory_used_bytes = mem_usage.used;
            usage.memory_total_bytes = mem_usage.total;
        }

        // Get disk usage
        if let Ok(disk_usage) = self.disk.get_usage().await {
            usage.disk_used_bytes = disk_usage.used;
            usage.disk_total_bytes = disk_usage.total;
        }

        // Emit metrics
        // metrics::gauge!("resource.memory.used_bytes", usage.memory_used_bytes as f64);
        // metrics::gauge!("resource.gpu.memory.used_bytes", usage.gpu_memory_used_bytes as f64);
        // metrics::gauge!("resource.gpu.temperature", usage.gpu_temperature as f64);
        // metrics::gauge!("resource.disk.used_bytes", usage.disk_used_bytes as f64);
    }

    /// Get current resource pressure
    pub async fn get_resource_pressure(&self) -> ResourcePressure {
        let usage = self.usage.read().await;

        let memory_ratio = usage.memory_used_bytes as f32 / usage.memory_total_bytes as f32;
        let gpu_memory_ratio = if usage.gpu_memory_total_bytes > 0 {
            usage.gpu_memory_used_bytes as f32 / usage.gpu_memory_total_bytes as f32
        } else {
            0.0
        };

        // Determine overall pressure
        let max_ratio = memory_ratio.max(gpu_memory_ratio);

        if max_ratio > self.config.memory.emergency_threshold {
            ResourcePressure::Critical
        } else if max_ratio > self.config.memory.critical_threshold {
            ResourcePressure::High
        } else if max_ratio > self.config.memory.warning_threshold {
            ResourcePressure::Medium
        } else if max_ratio > 0.5 {
            ResourcePressure::Low
        } else {
            ResourcePressure::None
        }
    }

    /// Handle resource pressure
    async fn handle_pressure(&self, pressure: ResourcePressure) {
        match pressure {
            ResourcePressure::Critical => {
                // Emergency measures
                self.memory.trigger_emergency_cleanup().await;
                self.gpu.throttle_compute(0.5).await;
            }
            ResourcePressure::High => {
                // Aggressive cleanup
                self.memory.trigger_critical_cleanup().await;
                self.gpu.reduce_batch_size().await;
            }
            ResourcePressure::Medium => {
                // Moderate cleanup
                self.memory.trigger_warning_cleanup().await;
            }
            _ => {}
        }
    }

    /// Get current resource usage
    pub async fn get_usage(&self) -> ResourceUsage {
        self.usage.read().await.clone()
    }

    /// Report metrics
    pub async fn report_metrics(&self) -> Result<(), SelfwareError> {
        let usage = self.get_usage().await;

        info!(
            memory_used_gb = usage.memory_used_bytes / 1_000_000_000,
            gpu_memory_used_gb = usage.gpu_memory_used_bytes / 1_000_000_000,
            gpu_temp = usage.gpu_temperature,
            disk_used_gb = usage.disk_used_bytes / 1_000_000_000,
            "Resource usage report"
        );

        Ok(())
    }

    /// Check if operation is within quotas
    pub async fn check_quotas(&self, required: &ResourceRequest) -> Result<(), ResourceError> {
        let quotas = self.quotas.read().await;
        quotas.check(required).await
    }

    /// Reserve resources for an operation
    pub async fn reserve(
        &self,
        request: ResourceRequest,
    ) -> Result<ResourceReservation, ResourceError> {
        self.check_quotas(&request).await?;

        Ok(ResourceReservation {
            request,
            reserved_at: std::time::Instant::now(),
        })
    }
}

/// Resource request
#[derive(Debug, Clone)]
pub struct ResourceRequest {
    pub gpu_memory_bytes: u64,
    pub system_memory_bytes: u64,
    pub disk_bytes: u64,
    pub duration_estimate: Duration,
}

/// Resource reservation
#[derive(Debug, Clone)]
pub struct ResourceReservation {
    pub request: ResourceRequest,
    pub reserved_at: std::time::Instant,
}

impl ResourceReservation {
    /// Release the reservation
    pub fn release(self) {
        // In a real implementation, this would update resource tracking
        debug!("Resource reservation released");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
