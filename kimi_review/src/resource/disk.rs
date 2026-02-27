//! Disk resource management

use crate::config::DiskConfig;
use crate::error::ResourceError;
use std::path::PathBuf;
use std::time::Duration;
use tokio::fs;
use tracing::{debug, info, warn};

/// Disk manager for storage management
pub struct DiskManager {
    config: DiskConfig,
    checkpoints_path: PathBuf,
    logs_path: PathBuf,
    models_path: PathBuf,
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
            models_path,
        })
    }
    
    /// Get disk usage
    pub async fn get_usage(&self) -> Result<DiskUsage, ResourceError> {
        use sysinfo::{DiskExt, System, SystemExt};
        
        let system = System::new_all();
        
        // Find the disk containing checkpoints
        for disk in system.disks() {
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
        
        for disk in system.disks() {
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
        
        Err(ResourceError::DiskExhausted("Could not determine disk usage".to_string()))
    }
    
    /// Start maintenance loop
    pub async fn maintenance_loop(&self) {
        let mut interval = tokio::time::interval(Duration::from_secs(self.config.maintenance_interval_seconds));
        
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
        
        let compress_after = Duration::from_secs(self.config.compress_after_days as u64 * 24 * 3600);
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
        let data = fs::read(path).await.map_err(|e| {
            ResourceError::DiskExhausted(format!("Failed to read file: {}", e))
        })?;
        
        let compressed = zstd::encode_all(&data[..], 6).map_err(|e| {
            ResourceError::DiskExhausted(format!("Failed to compress: {}", e))
        })?;
        
        let mut new_path = path.clone();
        new_path.set_extension("chk.zst");
        
        fs::write(&new_path, compressed).await.map_err(|e| {
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
