//! Disk resource management

use crate::config::DiskConfig;
use crate::errors::ResourceError;
use std::path::PathBuf;
use std::time::Duration;
use tokio::fs;
use tracing::{debug, info, warn};

/// Disk manager for storage management
pub struct DiskManager {
    config: DiskConfig,
    checkpoints_path: PathBuf,
    logs_path: PathBuf,
    _models_path: PathBuf,
}

/// Disk usage statistics
#[derive(Debug, Clone, Default)]
pub struct DiskUsage {
    pub used: u64,
    pub total: u64,
    pub available: u64,
    pub percent: f32,
}

/// Storage estimate for planning
#[derive(Debug, Clone)]
pub struct StorageEstimate {
    pub checkpoints: u64,
    pub logs: u64,
    pub models: u64,
    pub buffer: u64,
}

impl StorageEstimate {
    /// Total estimated storage needed
    pub fn total(&self) -> u64 {
        self.checkpoints + self.logs + self.models + self.buffer
    }
}

impl DiskManager {
    /// Create a new disk manager
    pub async fn new(config: &DiskConfig) -> Result<Self, ResourceError> {
        let checkpoints_path = std::env::var("SELFWARE_CHECKPOINT_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("./checkpoints"));

        let logs_path = std::env::var("SELFWARE_LOG_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("./logs"));

        let models_path = std::env::var("SELFWARE_MODEL_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("./models"));

        // Ensure directories exist
        for path in [&checkpoints_path, &logs_path, &models_path] {
            if !path.exists() {
                fs::create_dir_all(path).await.map_err(|e| {
                    ResourceError::DiskExhausted(format!("Failed to create directory: {}", e))
                })?;
            }
        }

        info!(
            checkpoints = %checkpoints_path.display(),
            logs = %logs_path.display(),
            models = %models_path.display(),
            "Disk manager initialized"
        );

        Ok(Self {
            config: config.clone(),
            checkpoints_path,
            logs_path,
            _models_path: models_path,
        })
    }

    /// Get disk usage
    pub async fn get_usage(&self) -> Result<DiskUsage, ResourceError> {
        use sysinfo::Disks;

        let disks = Disks::new_with_refreshed_list();

        // Find the disk containing checkpoints
        for disk in disks.list() {
            if self.checkpoints_path.starts_with(disk.mount_point()) {
                let total = disk.total_space();
                let available = disk.available_space();
                let used = total - available;

                return Ok(DiskUsage {
                    used,
                    total,
                    available,
                    percent: if total > 0 {
                        used as f32 / total as f32
                    } else {
                        0.0
                    },
                });
            }
        }

        // Fallback: use current directory
        let current = std::env::current_dir().map_err(|e| {
            ResourceError::DiskExhausted(format!("Failed to get current directory: {}", e))
        })?;

        for disk in disks.list() {
            if current.starts_with(disk.mount_point()) {
                let total = disk.total_space();
                let available = disk.available_space();
                let used = total - available;

                return Ok(DiskUsage {
                    used,
                    total,
                    available,
                    percent: if total > 0 {
                        used as f32 / total as f32
                    } else {
                        0.0
                    },
                });
            }
        }

        Err(ResourceError::DiskExhausted(
            "Could not determine disk usage".to_string(),
        ))
    }

    /// Start maintenance loop
    pub async fn maintenance_loop(&self) {
        let mut interval = tokio::time::interval(Duration::from_secs(
            self.config.maintenance_interval_seconds,
        ));

        loop {
            interval.tick().await;

            if let Err(e) = self.perform_maintenance().await {
                warn!(error = %e, "Maintenance failed");
            }
        }
    }

    /// Perform maintenance tasks
    async fn perform_maintenance(&self) -> Result<(), ResourceError> {
        debug!("Starting disk maintenance");

        // Check disk usage
        let usage = self.get_usage().await?;

        if usage.percent > self.config.max_usage_percent {
            warn!(percent = usage.percent, "Disk usage high, cleaning up");
            self.cleanup_old_files().await?;
        }

        // Compress old checkpoints
        self.compress_old_checkpoints().await?;

        // Clean up orphaned files
        self.cleanup_orphaned_files().await?;

        debug!("Disk maintenance completed");
        Ok(())
    }

    /// Clean up old files
    async fn cleanup_old_files(&self) -> Result<u64, ResourceError> {
        let mut deleted = 0u64;

        // Clean old logs
        if let Ok(mut entries) = fs::read_dir(&self.logs_path).await {
            let cutoff = std::time::SystemTime::now() - Duration::from_secs(7 * 24 * 3600);

            while let Ok(Some(entry)) = entries.next_entry().await {
                if let Ok(metadata) = entry.metadata().await {
                    if let Ok(modified) = metadata.modified() {
                        if modified < cutoff {
                            if let Err(e) = fs::remove_file(entry.path()).await {
                                debug!(path = %entry.path().display(), error = %e, "Failed to delete old log");
                            } else {
                                deleted += 1;
                            }
                        }
                    }
                }
            }
        }

        info!(deleted_files = deleted, "Old files cleaned up");
        Ok(deleted)
    }

    /// Compress old checkpoints
    async fn compress_old_checkpoints(&self) -> Result<u64, ResourceError> {
        let mut compressed = 0u64;

        let compress_after =
            Duration::from_secs(self.config.compress_after_days as u64 * 24 * 3600);
        let cutoff = std::time::SystemTime::now() - compress_after;

        if let Ok(mut entries) = fs::read_dir(&self.checkpoints_path).await {
            while let Ok(Some(entry)) = entries.next_entry().await {
                let path = entry.path();

                // Skip already compressed files
                if path.extension().map(|e| e == "zst").unwrap_or(false) {
                    continue;
                }

                if let Ok(metadata) = entry.metadata().await {
                    if let Ok(modified) = metadata.modified() {
                        if modified < cutoff {
                            // Compress the file
                            if let Err(e) = self.compress_file(&path).await {
                                debug!(path = %path.display(), error = %e, "Failed to compress checkpoint");
                            } else {
                                compressed += 1;
                            }
                        }
                    }
                }
            }
        }

        info!(compressed_files = compressed, "Old checkpoints compressed");
        Ok(compressed)
    }

    /// Compress a single file
    async fn compress_file(&self, path: &PathBuf) -> Result<(), ResourceError> {
        let data = fs::read(path)
            .await
            .map_err(|e| ResourceError::DiskExhausted(format!("Failed to read file: {}", e)))?;

        let compressed = zstd::encode_all(&data[..], 6)
            .map_err(|e| ResourceError::DiskExhausted(format!("Failed to compress: {}", e)))?;

        let mut new_path = path.clone();
        new_path.set_extension("chk.zst");

        fs::write(&new_path, &compressed).await.map_err(|e| {
            ResourceError::DiskExhausted(format!("Failed to write compressed file: {}", e))
        })?;

        fs::remove_file(path).await.map_err(|e| {
            ResourceError::DiskExhausted(format!("Failed to remove original file: {}", e))
        })?;

        Ok(())
    }

    /// Clean up orphaned files
    async fn cleanup_orphaned_files(&self) -> Result<u64, ResourceError> {
        // In a real implementation, this would check for files not referenced
        // by any checkpoint and remove them
        Ok(0)
    }

    /// Estimate storage needs for a run
    pub fn estimate_storage_needs(&self, days: u32) -> StorageEstimate {
        let daily_checkpoint_size = 500 * 1024 * 1024u64; // 500MB/day
        let daily_log_size = 100 * 1024 * 1024u64; // 100MB/day

        StorageEstimate {
            checkpoints: daily_checkpoint_size * days as u64,
            logs: daily_log_size * days as u64,
            models: self.get_models_size(),
            buffer: daily_checkpoint_size * 2, // 2-day buffer
        }
    }

    /// Get total size of models directory
    fn get_models_size(&self) -> u64 {
        // In a real implementation, this would recursively calculate directory size
        10_000_000_000 // 10GB placeholder
    }

    /// Get available space
    pub async fn available_space(&self) -> Result<u64, ResourceError> {
        let usage = self.get_usage().await?;
        Ok(usage.available)
    }

    /// Check if there's enough space for an operation
    pub async fn check_space(&self, required_bytes: u64) -> Result<bool, ResourceError> {
        let available = self.available_space().await?;
        Ok(available >= required_bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- StorageEstimate tests ----

    #[test]
    fn test_storage_estimate_total() {
        let estimate = StorageEstimate {
            checkpoints: 1_000_000,
            logs: 200_000,
            models: 5_000_000,
            buffer: 500_000,
        };
        assert_eq!(estimate.total(), 6_700_000);
    }

    #[test]
    fn test_storage_estimate_total_zero() {
        let estimate = StorageEstimate {
            checkpoints: 0,
            logs: 0,
            models: 0,
            buffer: 0,
        };
        assert_eq!(estimate.total(), 0);
    }

    #[test]
    fn test_storage_estimate_total_large_values() {
        let estimate = StorageEstimate {
            checkpoints: 100_000_000_000,
            logs: 50_000_000_000,
            models: 200_000_000_000,
            buffer: 10_000_000_000,
        };
        assert_eq!(estimate.total(), 360_000_000_000);
    }

    #[test]
    fn test_storage_estimate_clone() {
        let estimate = StorageEstimate {
            checkpoints: 42,
            logs: 7,
            models: 99,
            buffer: 13,
        };
        let cloned = estimate.clone();
        assert_eq!(cloned.checkpoints, 42);
        assert_eq!(cloned.total(), estimate.total());
    }

    // ---- DiskUsage tests ----

    #[test]
    fn test_disk_usage_default() {
        let usage = DiskUsage::default();
        assert_eq!(usage.used, 0);
        assert_eq!(usage.total, 0);
        assert_eq!(usage.available, 0);
        assert_eq!(usage.percent, 0.0);
    }

    #[test]
    fn test_disk_usage_clone() {
        let usage = DiskUsage {
            used: 500_000_000_000,
            total: 1_000_000_000_000,
            available: 500_000_000_000,
            percent: 0.5,
        };
        let cloned = usage.clone();
        assert_eq!(cloned.used, usage.used);
        assert_eq!(cloned.total, usage.total);
        assert_eq!(cloned.available, usage.available);
        assert_eq!(cloned.percent, usage.percent);
    }

    // ---- DiskManager estimation tests ----

    /// Helper to create a DiskManager without async, for unit testing estimation logic.
    fn make_test_disk_manager(config: &DiskConfig) -> DiskManager {
        DiskManager {
            config: config.clone(),
            checkpoints_path: PathBuf::from("/tmp/test_checkpoints"),
            logs_path: PathBuf::from("/tmp/test_logs"),
            _models_path: PathBuf::from("/tmp/test_models"),
        }
    }

    #[test]
    fn test_estimate_storage_needs_one_day() {
        let config = DiskConfig::default();
        let dm = make_test_disk_manager(&config);
        let estimate = dm.estimate_storage_needs(1);

        // 1 day: 500MB checkpoints + 100MB logs + 10GB models + 1GB buffer
        assert_eq!(estimate.checkpoints, 500 * 1024 * 1024);
        assert_eq!(estimate.logs, 100 * 1024 * 1024);
        assert_eq!(estimate.models, 10_000_000_000);
        assert_eq!(estimate.buffer, 500 * 1024 * 1024 * 2); // 2-day buffer
    }

    #[test]
    fn test_estimate_storage_needs_thirty_days() {
        let config = DiskConfig::default();
        let dm = make_test_disk_manager(&config);
        let estimate = dm.estimate_storage_needs(30);

        assert_eq!(estimate.checkpoints, 500 * 1024 * 1024 * 30);
        assert_eq!(estimate.logs, 100 * 1024 * 1024 * 30);
        // Buffer is always 2 days regardless of run length
        assert_eq!(estimate.buffer, 500 * 1024 * 1024 * 2);
    }

    #[test]
    fn test_estimate_storage_needs_zero_days() {
        let config = DiskConfig::default();
        let dm = make_test_disk_manager(&config);
        let estimate = dm.estimate_storage_needs(0);

        assert_eq!(estimate.checkpoints, 0);
        assert_eq!(estimate.logs, 0);
        // models and buffer are constant
        assert_eq!(estimate.models, 10_000_000_000);
        assert_eq!(estimate.buffer, 500 * 1024 * 1024 * 2);
    }

    #[test]
    fn test_estimate_storage_total_scales_with_days() {
        let config = DiskConfig::default();
        let dm = make_test_disk_manager(&config);
        let est1 = dm.estimate_storage_needs(1);
        let est10 = dm.estimate_storage_needs(10);

        // 10-day estimate should be larger than 1-day
        assert!(est10.total() > est1.total());
        // The variable parts (checkpoints + logs) should scale by 10x
        assert_eq!(est10.checkpoints, est1.checkpoints * 10);
        assert_eq!(est10.logs, est1.logs * 10);
    }

    #[test]
    fn test_get_models_size_returns_placeholder() {
        let config = DiskConfig::default();
        let dm = make_test_disk_manager(&config);
        // estimate_storage_needs calls get_models_size internally
        let estimate = dm.estimate_storage_needs(1);
        assert_eq!(estimate.models, 10_000_000_000);
    }

    #[test]
    fn test_disk_config_defaults_reasonable() {
        let config = DiskConfig::default();
        assert!(config.max_usage_percent > 0.0 && config.max_usage_percent <= 1.0);
        assert!(config.maintenance_interval_seconds > 0);
        assert!(config.compress_after_days > 0);
    }
}
