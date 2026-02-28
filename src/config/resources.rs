use serde::{Deserialize, Serialize};

/// Resource management configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
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

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // ========================================================================
    // GpuConfig defaults
    // ========================================================================

    #[test]
    fn test_gpu_config_default_values() {
        let gpu = GpuConfig::default();
        assert_eq!(gpu.monitor_interval_seconds, 5);
        assert_eq!(gpu.temperature_threshold, 85);
        assert!((gpu.memory_utilization_threshold - 0.95).abs() < f32::EPSILON);
        assert!(gpu.throttle_on_overheat);
    }

    #[test]
    fn test_gpu_config_temperature_threshold_sane_range() {
        let gpu = GpuConfig::default();
        // GPU throttle temperatures are typically 70-100 C
        assert!(
            gpu.temperature_threshold >= 60 && gpu.temperature_threshold <= 105,
            "Temperature threshold {} is outside sane range [60, 105]",
            gpu.temperature_threshold
        );
    }

    #[test]
    fn test_gpu_config_memory_utilization_threshold_sane_range() {
        let gpu = GpuConfig::default();
        assert!(
            gpu.memory_utilization_threshold > 0.0 && gpu.memory_utilization_threshold <= 1.0,
            "Memory utilization threshold {} must be in (0.0, 1.0]",
            gpu.memory_utilization_threshold
        );
    }

    #[test]
    fn test_gpu_config_monitor_interval_positive() {
        let gpu = GpuConfig::default();
        assert!(
            gpu.monitor_interval_seconds > 0,
            "Monitor interval must be positive"
        );
    }

    // ========================================================================
    // MemoryConfig defaults
    // ========================================================================

    #[test]
    fn test_memory_config_default_values() {
        let mem = MemoryConfig::default();
        assert!((mem.warning_threshold - 0.70).abs() < f32::EPSILON);
        assert!((mem.critical_threshold - 0.85).abs() < f32::EPSILON);
        assert!((mem.emergency_threshold - 0.95).abs() < f32::EPSILON);
        assert_eq!(mem.monitor_interval_seconds, 2);
    }

    #[test]
    fn test_memory_config_thresholds_are_ordered() {
        let mem = MemoryConfig::default();
        assert!(
            mem.warning_threshold < mem.critical_threshold,
            "Warning ({}) should be less than critical ({})",
            mem.warning_threshold,
            mem.critical_threshold
        );
        assert!(
            mem.critical_threshold < mem.emergency_threshold,
            "Critical ({}) should be less than emergency ({})",
            mem.critical_threshold,
            mem.emergency_threshold
        );
    }

    #[test]
    fn test_memory_config_thresholds_within_zero_to_one() {
        let mem = MemoryConfig::default();
        for (name, val) in [
            ("warning", mem.warning_threshold),
            ("critical", mem.critical_threshold),
            ("emergency", mem.emergency_threshold),
        ] {
            assert!(
                val > 0.0 && val <= 1.0,
                "{} threshold {} must be in (0.0, 1.0]",
                name,
                val
            );
        }
    }

    #[test]
    fn test_memory_config_monitor_interval_positive() {
        let mem = MemoryConfig::default();
        assert!(
            mem.monitor_interval_seconds > 0,
            "Monitor interval must be positive"
        );
    }

    // ========================================================================
    // DiskConfig defaults
    // ========================================================================

    #[test]
    fn test_disk_config_default_values() {
        let disk = DiskConfig::default();
        assert!((disk.max_usage_percent - 0.85).abs() < f32::EPSILON);
        assert_eq!(disk.maintenance_interval_seconds, 3600);
        assert_eq!(disk.compress_after_days, 1);
    }

    #[test]
    fn test_disk_config_max_usage_within_sane_range() {
        let disk = DiskConfig::default();
        assert!(
            disk.max_usage_percent > 0.0 && disk.max_usage_percent <= 1.0,
            "Max usage percent {} must be in (0.0, 1.0]",
            disk.max_usage_percent
        );
    }

    #[test]
    fn test_disk_config_maintenance_interval_positive() {
        let disk = DiskConfig::default();
        assert!(
            disk.maintenance_interval_seconds > 0,
            "Maintenance interval must be positive"
        );
    }

    #[test]
    fn test_disk_config_compress_after_days_positive() {
        let disk = DiskConfig::default();
        assert!(
            disk.compress_after_days >= 1,
            "Compress after days must be at least 1"
        );
    }

    // ========================================================================
    // ResourceQuotas defaults
    // ========================================================================

    #[test]
    fn test_resource_quotas_default_values() {
        let quotas = ResourceQuotas::default();
        assert_eq!(quotas.max_gpu_memory_per_model, 20_000_000_000);
        assert_eq!(quotas.max_concurrent_requests, 4);
        assert_eq!(quotas.max_context_tokens, 1_000_000);
        assert_eq!(quotas.max_queued_tasks, 1000);
        assert_eq!(quotas.max_checkpoint_size, 2_000_000_000);
    }

    #[test]
    fn test_resource_quotas_concurrent_requests_positive() {
        let quotas = ResourceQuotas::default();
        assert!(
            quotas.max_concurrent_requests > 0,
            "Max concurrent requests must be positive"
        );
    }

    #[test]
    fn test_resource_quotas_context_tokens_positive() {
        let quotas = ResourceQuotas::default();
        assert!(
            quotas.max_context_tokens > 0,
            "Max context tokens must be positive"
        );
    }

    #[test]
    fn test_resource_quotas_gpu_memory_is_reasonable() {
        let quotas = ResourceQuotas::default();
        // Should be at least 1GB and at most 1TB
        assert!(
            quotas.max_gpu_memory_per_model >= 1_000_000_000,
            "GPU memory quota should be at least 1GB"
        );
        assert!(
            quotas.max_gpu_memory_per_model <= 1_000_000_000_000,
            "GPU memory quota should be at most 1TB"
        );
    }

    // ========================================================================
    // ResourcesConfig default composition
    // ========================================================================

    #[test]
    fn test_resources_config_default_composes_sub_configs() {
        let config = ResourcesConfig::default();

        // Verify it composes the same defaults as the individual sub-configs
        let gpu = GpuConfig::default();
        let mem = MemoryConfig::default();
        let disk = DiskConfig::default();
        let quotas = ResourceQuotas::default();

        // GPU
        assert_eq!(
            config.gpu.monitor_interval_seconds,
            gpu.monitor_interval_seconds
        );
        assert_eq!(config.gpu.temperature_threshold, gpu.temperature_threshold);
        assert!(
            (config.gpu.memory_utilization_threshold - gpu.memory_utilization_threshold).abs()
                < f32::EPSILON
        );
        assert_eq!(config.gpu.throttle_on_overheat, gpu.throttle_on_overheat);

        // Memory
        assert!((config.memory.warning_threshold - mem.warning_threshold).abs() < f32::EPSILON);
        assert!((config.memory.critical_threshold - mem.critical_threshold).abs() < f32::EPSILON);
        assert!((config.memory.emergency_threshold - mem.emergency_threshold).abs() < f32::EPSILON);
        assert_eq!(
            config.memory.monitor_interval_seconds,
            mem.monitor_interval_seconds
        );

        // Disk
        assert!((config.disk.max_usage_percent - disk.max_usage_percent).abs() < f32::EPSILON);
        assert_eq!(
            config.disk.maintenance_interval_seconds,
            disk.maintenance_interval_seconds
        );
        assert_eq!(config.disk.compress_after_days, disk.compress_after_days);

        // Quotas
        assert_eq!(
            config.quotas.max_gpu_memory_per_model,
            quotas.max_gpu_memory_per_model
        );
        assert_eq!(
            config.quotas.max_concurrent_requests,
            quotas.max_concurrent_requests
        );
        assert_eq!(config.quotas.max_context_tokens, quotas.max_context_tokens);
        assert_eq!(config.quotas.max_queued_tasks, quotas.max_queued_tasks);
        assert_eq!(
            config.quotas.max_checkpoint_size,
            quotas.max_checkpoint_size
        );
    }

    #[test]
    fn test_resources_config_serialization_roundtrip() {
        let config = ResourcesConfig::default();
        let json = serde_json::to_string(&config).expect("serialize");
        let deserialized: ResourcesConfig = serde_json::from_str(&json).expect("deserialize");

        // Spot-check a few fields survive the roundtrip
        assert_eq!(
            deserialized.gpu.temperature_threshold,
            config.gpu.temperature_threshold
        );
        assert!(
            (deserialized.memory.warning_threshold - config.memory.warning_threshold).abs()
                < f32::EPSILON
        );
        assert_eq!(
            deserialized.quotas.max_context_tokens,
            config.quotas.max_context_tokens
        );
    }
}
