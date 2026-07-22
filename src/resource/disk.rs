//! Disk resource management

use crate::config::DiskConfig;
use crate::errors::ResourceError;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::fs;
use tracing::{debug, info, warn};

/// Create a state directory at the point of first write.
///
/// `DiskManager::new` deliberately does NOT create its checkpoints/logs/models
/// directories up front: every CLI command (even read-only ones like
/// `selfware config show` or `selfware unpack --scan`) constructs a
/// `DiskManager` via the resource monitor, and eager creation polluted the
/// user's working directory with empty `checkpoints/`, `logs/`, `models/`
/// dirs. Creation happens here instead, only when something actually writes.
async fn ensure_dir(path: &Path) -> Result<(), ResourceError> {
    fs::create_dir_all(path).await.map_err(|e| {
        ResourceError::DiskExhausted(format!(
            "Failed to create directory {}: {}",
            path.display(),
            e
        ))
    })
}

/// Disk manager for storage management
pub struct DiskManager {
    config: DiskConfig,
    checkpoints_path: PathBuf,
    logs_path: PathBuf,
    models_path: PathBuf,
    /// Cache for models directory size: (calculated_at, size_in_bytes)
    models_size_cache: std::sync::Mutex<Option<(std::time::SystemTime, u64)>>,
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
    /// Create a new disk manager.
    ///
    /// Resolves the state-directory paths (env overrides or `./checkpoints`,
    /// `./logs`, `./models` defaults) but does NOT create them: directories
    /// are created lazily at the point of first write (see `ensure_dir`),
    /// so read-only commands never pollute the working directory.
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

        info!(
            checkpoints = %checkpoints_path.display(),
            logs = %logs_path.display(),
            models = %models_path.display(),
            "Disk manager initialized"
        );

        Ok(Self::with_paths(
            config,
            checkpoints_path,
            logs_path,
            models_path,
        ))
    }

    /// Construct a manager over explicit state-directory paths.
    ///
    /// The directories are not created here; see [`ensure_dir`].
    fn with_paths(
        config: &DiskConfig,
        checkpoints_path: PathBuf,
        logs_path: PathBuf,
        models_path: PathBuf,
    ) -> Self {
        Self {
            config: config.clone(),
            checkpoints_path,
            logs_path,
            models_path,
            models_size_cache: std::sync::Mutex::new(None),
        }
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

        // Lazy state-dir creation: the target directory is created at the
        // point of first write, never at startup. (In practice the dir
        // already exists because `path` was found inside it.)
        if let Some(parent) = new_path.parent() {
            ensure_dir(parent).await?;
        }

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

    /// Get total size of models directory.
    ///
    /// Recursively calculates the total file size under the models directory.
    /// Results are cached for a short TTL to avoid repeated expensive traversals.
    ///
    /// # Behavior
    ///
    /// - If the models directory exists, its size is calculated by walking the
    ///   directory tree and summing regular file sizes.
    /// - Symlinks are **skipped** to avoid counting mount points twice or
    ///   entering cycles.
    /// - If the directory does not exist or cannot be read, a fallback estimate
    ///   of 10 GB is returned so that storage planning is not blocked.
    /// - Results are cached for 60 seconds to amortize the cost of repeated
    ///   calls during a single estimation pass.
    ///
    /// # Returns
    ///
    /// Total size in bytes of all regular files under the models directory,
    /// or a 10 GB fallback (`10_000_000_000`) when the directory is absent.
    fn get_models_size(&self) -> u64 {
        const CACHE_TTL: Duration = Duration::from_secs(60);
        const FALLBACK_SIZE: u64 = 10_000_000_000;

        // Return cached value if still fresh
        if let Ok(cache) = self.models_size_cache.lock() {
            if let Some((cached_at, cached_size)) = *cache {
                if cached_at.elapsed().unwrap_or(CACHE_TTL * 2) < CACHE_TTL {
                    debug!(size = cached_size, "Returning cached models directory size");
                    return cached_size;
                }
            }
        }

        // Calculate actual directory size
        let size = self.calculate_dir_size(&self.models_path);

        let size = if size == 0 && !self.models_path.exists() {
            warn!(
                path = %self.models_path.display(),
                "Models directory does not exist, returning 10GB fallback estimate"
            );
            FALLBACK_SIZE
        } else {
            debug!(
                size,
                path = %self.models_path.display(),
                "Calculated models directory size"
            );
            size
        };

        // Update cache
        if let Ok(mut cache) = self.models_size_cache.lock() {
            *cache = Some((std::time::SystemTime::now(), size));
        }

        size
    }

    /// Recursively calculate the total size of all regular files under `path`.
    ///
    /// Symlinks are skipped to avoid cycles and double-counting across mount
    /// points.  Errors on individual entries are silently skipped so that a
    /// single unreadable file does not abort the entire traversal.
    fn calculate_dir_size(&self, path: &PathBuf) -> u64 {
        let mut total = 0u64;

        let entries = match std::fs::read_dir(path) {
            Ok(e) => e,
            Err(_) => return 0,
        };

        for entry in entries.flatten() {
            let file_type = match entry.file_type() {
                Ok(ft) => ft,
                Err(_) => continue,
            };

            // Skip symlinks to avoid cycles and double-counting mount points
            if file_type.is_symlink() {
                continue;
            }

            if file_type.is_file() {
                if let Ok(metadata) = entry.metadata() {
                    total += metadata.len();
                }
            } else if file_type.is_dir() {
                total += self.calculate_dir_size(&entry.path());
            }
        }

        total
    }

    /// Get available space
    pub async fn available_space(&self) -> Result<u64, ResourceError> {
        let usage = self.get_usage().await?;
        Ok(usage.available)
    }

}

#[cfg(test)]
#[path = "../../tests/unit/resource/disk/disk_test.rs"]
mod tests;
