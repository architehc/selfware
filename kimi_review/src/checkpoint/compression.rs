//! Compression engine for checkpoints

use crate::error::CheckpointError;
use std::io::{Read, Write};

/// Compression engine for checkpoint data
pub struct CompressionEngine {
    algorithm: CompressionAlgorithm,
    level: u32,
}

/// Compression algorithm
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompressionAlgorithm {
    None,
    Zstd,
    Gzip,
}

impl CompressionEngine {
    /// Create a new compression engine
    pub fn new(algorithm: CompressionAlgorithm, level: u32) -> Self {
        Self { algorithm, level }
    }
    
    /// Compress data
    pub fn compress(&self, data: &[u8]) -> Result<Vec<u8>, CheckpointError> {
        match self.algorithm {
            CompressionAlgorithm::None => Ok(data.to_vec()),
            CompressionAlgorithm::Zstd => {
                zstd::encode_all(data, self.level as i32)
                    .map_err(|e| CheckpointError::Compression(e.to_string()))
            }
            CompressionAlgorithm::Gzip => {
                use flate2::write::GzEncoder;
                use flate2::Compression;
                
                let mut encoder = GzEncoder::new(Vec::new(), Compression::new(self.level));
                encoder.write_all(data).map_err(|e| {
                    CheckpointError::Compression(e.to_string())
                })?;
                encoder.finish().map_err(|e| {
                    CheckpointError::Compression(e.to_string())
                })
            }
        }
    }
    
    /// Decompress data
    pub fn decompress(&self, data: &[u8]) -> Result<Vec<u8>, CheckpointError> {
        match self.algorithm {
            CompressionAlgorithm::None => Ok(data.to_vec()),
            CompressionAlgorithm::Zstd => {
                zstd::decode_all(data)
                    .map_err(|e| CheckpointError::Compression(e.to_string()))
            }
            CompressionAlgorithm::Gzip => {
                use flate2::read::GzDecoder;
                
                let mut decoder = GzDecoder::new(data);
                let mut result = Vec::new();
                decoder.read_to_end(&mut result).map_err(|e| {
                    CheckpointError::Compression(e.to_string())
                })?;
                Ok(result)
            }
        }
    }
    
    /// Auto-detect and decompress
    pub fn auto_decompress(data: &[u8]) -> Result<Vec<u8>, CheckpointError> {
        // Try zstd magic number
        if data.starts_with(&[0x28, 0xB5, 0x2F, 0xFD]) {
            return zstd::decode_all(data)
                .map_err(|e| CheckpointError::Compression(e.to_string()));
        }
        
        // Try gzip magic number
        if data.starts_with(&[0x1F, 0x8B]) {
            use flate2::read::GzDecoder;
            let mut decoder = GzDecoder::new(data);
            let mut result = Vec::new();
            decoder.read_to_end(&mut result).map_err(|e| {
                CheckpointError::Compression(e.to_string())
            })?;
            return Ok(result);
        }
        
        // Assume uncompressed
        Ok(data.to_vec())
    }
}

impl Default for CompressionEngine {
    fn default() -> Self {
        Self {
            algorithm: CompressionAlgorithm::Zstd,
            level: 6,
        }
    }
}
