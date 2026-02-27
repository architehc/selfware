//! Checkpoint storage implementation

use super::{Checkpoint, CheckpointId, CheckpointMetadata};
use crate::config::{CheckpointConfig, CompressionAlgorithm};
use crate::error::{CheckpointError, SelfwareError};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::fs;
use tokio::io::AsyncWriteExt;
use tracing::{debug, error, info, warn};

/// Checkpoint storage backend
pub struct CheckpointStorage {
    config: CheckpointConfig,
    base_path: PathBuf,
}

impl CheckpointStorage {
    /// Create new checkpoint storage
    pub async fn new(config: &CheckpointConfig) -> Result<Self, SelfwareError> {
        let base_path = config.storage_path.clone();
        
        // Ensure storage directory exists
        fs::create_dir_all(&base_path).await.map_err(|e| {
            CheckpointError::Storage(format!("Failed to create storage directory: {}", e))
        })?;
        
        // Create subdirectories
        for subdir in &["checkpoints", "chunks", "journal"] {
            fs::create_dir_all(base_path.join(subdir)).await.map_err(|e| {
                CheckpointError::Storage(format!("Failed to create subdirectory: {}", e))
            })?;
        }
        
        info!(path = %base_path.display(), "Checkpoint storage initialized");
        
        Ok(Self {
            config: config.clone(),
            base_path,
        })
    }
    
    /// Store a checkpoint
    pub async fn store(&self, checkpoint: &Checkpoint) -> Result<(), SelfwareError> {
        let path = self.checkpoint_path(&checkpoint.id);
        
        // Serialize checkpoint
        let data = bincode::serialize(checkpoint).map_err(|e| {
            CheckpointError::Serialization(Box::new(e))
        })?;
        
        // Compress if enabled
        let (data, compressed) = if self.should_compress() {
            let compressed = self.compress(&data).await?;
            (compressed, true)
        } else {
            (data, false)
        };
        
        // Write to file
        let mut file = fs::File::create(&path).await.map_err(|e| {
            CheckpointError::Storage(format!("Failed to create checkpoint file: {}", e))
        })?;
        
        file.write_all(&data).await.map_err(|e| {
            CheckpointError::Storage(format!("Failed to write checkpoint: {}", e))
        })?;
        
        file.sync_all().await.map_err(|e| {
            CheckpointError::Storage(format!("Failed to sync checkpoint: {}", e))
        })?;
        
        debug!(
            checkpoint_id = %checkpoint.id,
            path = %path.display(),
            size_bytes = data.len(),
            compressed = compressed,
            "Checkpoint stored"
        );
        
        Ok(())
    }
    
    /// Load a checkpoint by ID
    pub async fn load(&self, id: &CheckpointId) -> Result<Checkpoint, SelfwareError> {
        let path = self.checkpoint_path(id);
        
        // Read file
        let data = fs::read(&path).await.map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                CheckpointError::NotFound(id.to_string())
            } else {
                CheckpointError::Storage(format!("Failed to read checkpoint: {}", e))
            }
        })?;
        
        // Try to decompress
        let data = if self.is_compressed(&data) {
            self.decompress(&data).await?
        } else {
            data
        };
        
        // Deserialize
        let checkpoint: Checkpoint = bincode::deserialize(&data).map_err(|e| {
            CheckpointError::Corrupted(format!("Failed to deserialize checkpoint: {}", e))
        })?;
        
        debug!(checkpoint_id = %id, "Checkpoint loaded");
        
        Ok(checkpoint)
    }
    
    /// List all checkpoints
    pub async fn list_checkpoints(&self) -> Result<Vec<CheckpointMetadata>, SelfwareError> {
        let mut checkpoints = Vec::new();
        let checkpoints_dir = self.base_path.join("checkpoints");
        
        let mut entries = fs::read_dir(&checkpoints_dir).await.map_err(|e| {
            CheckpointError::Storage(format!("Failed to read checkpoints directory: {}", e))
        })?;
        
        while let Some(entry) = entries.next_entry().await.map_err(|e| {
            CheckpointError::Storage(format!("Failed to read directory entry: {}", e))
        })? {
            let path = entry.path();
            
            if path.extension().map(|e| e == "chk").unwrap_or(false) {
                let metadata = fs::metadata(&path).await.map_err(|e| {
                    CheckpointError::Storage(format!("Failed to read file metadata: {}", e))
                })?;
                
                // Extract checkpoint ID from filename
                if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                    let id = CheckpointId::new(); // Parse from stem in real impl
                    
                    checkpoints.push(CheckpointMetadata {
                        id,
                        timestamp: chrono::DateTime::from(
                            std::time::UNIX_EPOCH + std::time::Duration::from_secs(
                                metadata.modified()
                                    .unwrap_or(std::time::SystemTime::UNIX_EPOCH)
                                    .duration_since(std::time::UNIX_EPOCH)
                                    .unwrap_or_default()
                                    .as_secs()
                            )
                        ),
                        level: super::CheckpointLevel::Session, // Would be extracted from file
                        size_bytes: metadata.len(),
                        compressed: self.is_compressed_file(&path),
                    });
                }
            }
        }
        
        // Sort by timestamp (newest first)
        checkpoints.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
        
        Ok(checkpoints)
    }
    
    /// Delete old checkpoints
    pub async fn cleanup_old(&self, retention_days: u32) -> Result<u64, SelfwareError> {
        let cutoff = chrono::Utc::now() - chrono::Duration::days(retention_days as i64);
        let mut deleted = 0u64;
        
        let checkpoints = self.list_checkpoints().await?;
        
        for checkpoint in checkpoints {
            if checkpoint.timestamp < cutoff {
                let path = self.checkpoint_path(&checkpoint.id);
                
                match fs::remove_file(&path).await {
                    Ok(_) => {
                        deleted += 1;
                        debug!(checkpoint_id = %checkpoint.id, "Deleted old checkpoint");
                    }
                    Err(e) => {
                        warn!(checkpoint_id = %checkpoint.id, error = %e, "Failed to delete checkpoint");
                    }
                }
            }
        }
        
        info!(deleted_count = deleted, "Checkpoint cleanup completed");
        
        Ok(deleted)
    }
    
    /// Flush all pending writes
    pub async fn flush(&self) -> Result<(), SelfwareError> {
        // In a real implementation, this would flush any buffered writes
        debug!("Storage flush completed");
        Ok(())
    }
    
    /// Get the path for a checkpoint
    fn checkpoint_path(&self, id: &CheckpointId) -> PathBuf {
        self.base_path
            .join("checkpoints")
            .join(format!("{}.chk", id))
    }
    
    /// Check if compression should be used
    fn should_compress(&self) -> bool {
        matches!(self.config.compression, CompressionAlgorithm::Zstd | CompressionAlgorithm::Gzip)
    }
    
    /// Compress data
    async fn compress(&self, data: &[u8]) -> Result<Vec<u8>, SelfwareError> {
        match self.config.compression {
            CompressionAlgorithm::Zstd => {
                zstd::encode_all(data, self.config.compression_level as i32)
                    .map_err(|e| CheckpointError::Compression(e.to_string()).into())
            }
            CompressionAlgorithm::Gzip => {
                use flate2::write::GzEncoder;
                use flate2::Compression;
                use std::io::Write;
                
                let mut encoder = GzEncoder::new(Vec::new(), Compression::new(self.config.compression_level));
                encoder.write_all(data).map_err(|e| {
                    CheckpointError::Compression(e.to_string())
                })?;
                encoder.finish().map_err(|e| {
                    CheckpointError::Compression(e.to_string())
                }).map_err(Into::into)
            }
            _ => Ok(data.to_vec()),
        }
    }
    
    /// Decompress data
    async fn decompress(&self, data: &[u8]) -> Result<Vec<u8>, SelfwareError> {
        // Try zstd first
        if let Ok(decompressed) = zstd::decode_all(data) {
            return Ok(decompressed);
        }
        
        // Try gzip
        use flate2::read::GzDecoder;
        use std::io::Read;
        
        let mut decoder = GzDecoder::new(data);
        let mut result = Vec::new();
        
        if decoder.read_to_end(&mut result).is_ok() {
            return Ok(result);
        }
        
        // Assume uncompressed
        Ok(data.to_vec())
    }
    
    /// Check if data is compressed (simple heuristic)
    fn is_compressed(&self, data: &[u8]) -> bool {
        // Zstd magic number
        if data.starts_with(&[0x28, 0xB5, 0x2F, 0xFD]) {
            return true;
        }
        // Gzip magic number
        if data.starts_with(&[0x1F, 0x8B]) {
            return true;
        }
        false
    }
    
    /// Check if file is compressed based on extension
    fn is_compressed_file(&self, path: &PathBuf) -> bool {
        path.extension()
            .and_then(|e| e.to_str())
            .map(|e| e == "zst" || e == "gz")
            .unwrap_or(false)
    }
}
