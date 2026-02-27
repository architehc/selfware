use serde::{Deserialize, Serialize};

/// Resource management configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[derive(Default)]
pub struct ResourcesConfig {
    #[serde(default)]
    pub gpu: GpuConfig,
    #[serde(default)]
    pub memory: MemoryConfig,
    #[serde(default)]
    pub disk: DiskConfig,
    #[serde(default)]
    pub quotas: ResourceQuotas,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpuConfig {
    pub monitor_interval_seconds: u64,
    pub temperature_threshold: u32,
    pub memory_utilization_threshold: f32,
    pub throttle_on_overheat: bool,
}

impl Default for GpuConfig {
    fn default() -> Self {
        Self {
            monitor_interval_seconds: 5,
            temperature_threshold: 85,
            memory_utilization_threshold: 0.95,
            throttle_on_overheat: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryConfig {
    pub warning_threshold: f32,
    pub critical_threshold: f32,
    pub emergency_threshold: f32,
    pub monitor_interval_seconds: u64,
}

impl Default for MemoryConfig {
    fn default() -> Self {
        Self {
            warning_threshold: 0.70,
            critical_threshold: 0.85,
            emergency_threshold: 0.95,
            monitor_interval_seconds: 2,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiskConfig {
    pub max_usage_percent: f32,
    pub maintenance_interval_seconds: u64,
    pub compress_after_days: u32,
}

impl Default for DiskConfig {
    fn default() -> Self {
        Self {
            max_usage_percent: 0.85,
            maintenance_interval_seconds: 3600,
            compress_after_days: 1,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceQuotas {
    pub max_gpu_memory_per_model: u64,
    pub max_concurrent_requests: usize,
    pub max_context_tokens: usize,
    pub max_queued_tasks: usize,
    pub max_checkpoint_size: u64,
}

impl Default for ResourceQuotas {
    fn default() -> Self {
        Self {
            max_gpu_memory_per_model: 20_000_000_000, // 20GB
            max_concurrent_requests: 4,
            max_context_tokens: 1_000_000,
            max_queued_tasks: 1000,
            max_checkpoint_size: 2_000_000_000, // 2GB
        }
    }
}

