//! Incremental checkpointing with content-defined chunking

use super::{Checkpoint, CheckpointId, CheckpointStorage, SessionCheckpointState};
use crate::error::CheckpointError;
use blake3::Hash;
use std::collections::HashMap;
use std::sync::Arc;
use tracing::debug;

/// Incremental checkpoint manager using content-defined chunking
pub struct IncrementalCheckpointManager {
    storage: Arc<CheckpointStorage>,
    chunk_cache: HashMap<Hash, Vec<u8>>,
}

/// A content chunk
#[derive(Debug, Clone)]
pub struct Chunk {
    pub hash: Hash,
    pub data: Vec<u8>,
}

/// Chunk reference in a checkpoint
#[derive(Debug, Clone)]
pub struct ChunkRef {
    pub hash: Hash,
    pub offset: usize,
    pub length: usize,
}

impl IncrementalCheckpointManager {
    /// Create a new incremental checkpoint manager
    pub fn new(storage: Arc<CheckpointStorage>) -> Self {
        Self {
            storage,
            chunk_cache: HashMap::new(),
        }
    }
    
    /// Create an incremental checkpoint
    pub async fn create_incremental(
        &mut self,
        previous: Option<&Checkpoint>,
        current: &SessionCheckpointState,
    ) -> Result<Checkpoint, CheckpointError> {
        let checkpoint_id = CheckpointId::new();
        
        // Serialize current state
        let serialized = bincode::serialize(current).map_err(|e| {
            CheckpointError::CreationFailed(format!("Serialization failed: {}", e))
        })?;
        
        // Content-defined chunking
        let chunks = self.chunk_data(&serialized);
        
        // Determine which chunks are new
        let new_chunks: Vec<_> = if let Some(prev) = previous {
            let prev_chunks = self.extract_chunks(prev).await?;
            chunks
                .into_iter()
                .filter(|c| !prev_chunks.contains(&c.hash))
                .collect()
        } else {
            chunks
        };
        
        // Store new chunks
        for chunk in &new_chunks {
            self.store_chunk(chunk).await?;
        }
        
        // Create checkpoint with chunk references
        let chunk_refs: Vec<_> = new_chunks
            .iter()
            .map(|c| ChunkRef {
                hash: c.hash,
                offset: 0,
                length: c.data.len(),
            })
            .collect();
        
        debug!(
            checkpoint_id = %checkpoint_id,
            new_chunks = new_chunks.len(),
            "Incremental checkpoint created"
        );
        
        Ok(Checkpoint {
            id: checkpoint_id,
            timestamp: chrono::Utc::now(),
            level: super::CheckpointLevel::Session,
            state: super::CheckpointState::Session(current.clone()),
            parent: previous.map(|c| c.id.clone()),
            diff_from_parent: Some(bincode::serialize(&chunk_refs).unwrap_or_default()),
        })
    }
    
    /// Chunk data using content-defined chunking
    fn chunk_data(&self, data: &[u8]) -> Vec<Chunk> {
        use fastcdc::FastCDC;
        
        let chunker = FastCDC::new(
            data,
            4096,      // min chunk size
            8192,      // avg chunk size
            16384,     // max chunk size
        );
        
        chunker
            .map(|chunk| {
                let data = chunk.data.to_vec();
                Chunk {
                    hash: blake3::hash(&data),
                    data,
                }
            })
            .collect()
    }
    
    /// Extract chunk hashes from a checkpoint
    async fn extract_chunks(&self, checkpoint: &Checkpoint) -> Result<Vec<Hash>, CheckpointError> {
        if let Some(diff) = &checkpoint.diff_from_parent {
            let refs: Vec<ChunkRef> = bincode::deserialize(diff).map_err(|e| {
                CheckpointError::Corrupted(format!("Failed to deserialize chunk refs: {}", e))
            })?;
            Ok(refs.into_iter().map(|r| r.hash).collect())
        } else {
            Ok(Vec::new())
        }
    }
    
    /// Store a chunk
    async fn store_chunk(&mut self, chunk: &Chunk) -> Result<(), CheckpointError> {
        // Store in cache
        self.chunk_cache.insert(chunk.hash, chunk.data.clone());
        
        // In a real implementation, persist to chunk store
        debug!(chunk_hash = %chunk.hash, chunk_size = chunk.data.len(), "Chunk stored");
        
        Ok(())
    }
    
    /// Reconstruct checkpoint from chunks
    pub async fn reconstruct(&self, checkpoint: &Checkpoint) -> Result<Vec<u8>, CheckpointError> {
        if let Some(diff) = &checkpoint.diff_from_parent {
            let refs: Vec<ChunkRef> = bincode::deserialize(diff).map_err(|e| {
                CheckpointError::Corrupted(format!("Failed to deserialize chunk refs: {}", e))
            })?;
            
            let mut data = Vec::new();
            for chunk_ref in refs {
                if let Some(chunk_data) = self.chunk_cache.get(&chunk_ref.hash) {
                    data.extend_from_slice(chunk_data);
                } else {
                    // Load from persistent store
                    return Err(CheckpointError::NotFound(
                        format!("Chunk not found: {}", chunk_ref.hash)
                    ));
                }
            }
            
            Ok(data)
        } else {
            // Full checkpoint, return as-is
            bincode::serialize(&checkpoint.state).map_err(|e| {
                CheckpointError::Serialization(Box::new(e))
            })
        }
    }
    
    /// Calculate storage savings from incremental checkpointing
    pub fn calculate_savings(&self, full_size: usize, incremental_size: usize) -> f64 {
        if full_size == 0 {
            0.0
        } else {
            (full_size - incremental_size) as f64 / full_size as f64
        }
    }
}
