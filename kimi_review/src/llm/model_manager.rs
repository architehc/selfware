//! Model lifecycle management for LLMs

use super::{LLMEngine, ModelConfig};
use crate::config::LLMConfig;
use crate::error::{LLMError, SelfwareError};
use crate::resource::gpu::{GpuManager, QuantizationLevel};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

/// Unique model identifier
pub type ModelId = crate::Id;

/// Model instance
pub struct ModelInstance {
    pub id: ModelId,
    pub state: ModelState,
    pub config: ModelConfig,
    pub engine: Arc<dyn LLMEngine>,
    pub last_used: RwLock<Instant>,
    pub use_count: AtomicU64,
    pub loaded_at: Instant,
}

impl std::fmt::Debug for ModelInstance {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ModelInstance")
            .field("id", &self.id)
            .field("state", &self.state)
            .field("config", &self.config)
            .field("loaded_at", &self.loaded_at)
            .finish()
    }
}

/// Model state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelState {
    Loading,
    Ready,
    Unloading,
    Offloaded,
    Error,
}

/// Model cache entry for CPU offloading
#[derive(Debug, Clone)]
pub struct ModelCacheEntry {
    pub model_id: ModelId,
    pub cpu_weights: Vec<u8>,
    pub cached_at: Instant,
}

/// Model lifecycle manager
pub struct ModelLifecycleManager {
    config: LLMConfig,
    models: Arc<RwLock<HashMap<ModelId, Arc<ModelInstance>>>>,
    gpu_manager: Arc<GpuManager>,
    cpu_cache: Arc<RwLock<HashMap<ModelId, ModelCacheEntry>>>,
    active_model: RwLock<Option<ModelId>>,
}

impl ModelLifecycleManager {
    /// Create a new model lifecycle manager
    pub async fn new(config: &LLMConfig) -> Result<Self, SelfwareError> {
        let gpu_manager = Arc::new(GpuManager::new(&Default::default()).await?);
        
        Ok(Self {
            config: config.clone(),
            models: Arc::new(RwLock::new(HashMap::new())),
            gpu_manager,
            cpu_cache: Arc::new(RwLock::new(HashMap::new())),
            active_model: RwLock::new(None),
        })
    }
    
    /// Load a model
    pub async fn load_model(&self, config: ModelConfig) -> Result<ModelId, LLMError> {
        let model_id = ModelId::new();
        
        info!(model_id = %model_id, path = %config.model_path.display(), "Loading model");
        
        // Check available memory
        let available = self.gpu_manager.get_available_memory().await;
        let required = self.estimate_memory(&config).await;
        
        // Adjust quantization if needed
        let adjusted_config = if available < required {
            warn!(
                available_gb = available / 1_000_000_000,
                required_gb = required / 1_000_000_000,
                "Insufficient GPU memory, adjusting quantization"
            );
            
            let quant = self.gpu_manager.adjust_quantization(required).await;
            
            ModelConfig {
                quantization: quant,
                ..config
            }
        } else {
            config
        };
        
        // Load the model
        let engine = self.create_engine(&adjusted_config).await?;
        
        let instance = Arc::new(ModelInstance {
            id: model_id.clone(),
            state: ModelState::Ready,
            config: adjusted_config,
            engine,
            last_used: RwLock::new(Instant::now()),
            use_count: AtomicU64::new(0),
            loaded_at: Instant::now(),
        });
        
        // Store model
        self.models.write().await.insert(model_id.clone(), instance);
        *self.active_model.write().await = Some(model_id.clone());
        
        info!(model_id = %model_id, "Model loaded successfully");
        
        Ok(model_id)
    }
    
    /// Unload a model
    pub async fn unload_model(&self, model_id: &ModelId) -> Result<(), LLMError> {
        info!(model_id = %model_id, "Unloading model");
        
        let mut models = self.models.write().await;
        
        if let Some(instance) = models.remove(model_id) {
            // Update state
            // In a real implementation, this would properly unload from GPU
            
            // Remove from active
            let mut active = self.active_model.write().await;
            if active.as_ref() == Some(model_id) {
                *active = None;
            }
            
            info!(model_id = %model_id, "Model unloaded");
        }
        
        Ok(())
    }
    
    /// Get a model instance
    pub async fn get_model(&self, model_id: &ModelId) -> Option<Arc<ModelInstance>> {
        let models = self.models.read().await;
        models.get(model_id).cloned()
    }
    
    /// Get the active model
    pub async fn get_active_model(&self) -> Option<Arc<ModelInstance>> {
        let active_id = self.active_model.read().await.clone()?;
        self.get_model(&active_id).await
    }
    
    /// Unload least recently used model
    pub async fn unload_lru(&self, required_bytes: u64) -> Result<(), LLMError> {
        let models = self.models.read().await;
        
        // Find LRU non-critical model
        let lru: Option<ModelId> = models
            .values()
            .filter(|m| !m.config.critical && m.state == ModelState::Ready)
            .min_by_key(|m| *m.last_used.read().await)
            .map(|m| m.id.clone());
        
        drop(models);
        
        if let Some(id) = lru {
            info!(model_id = %id, "Unloading LRU model");
            
            // Pre-load to CPU cache before unloading
            self.preload_to_cpu(&id).await?;
            
            self.unload_model(&id).await?;
            Ok(())
        } else {
            Err(LLMError::OutOfMemory)
        }
    }
    
    /// Swap models - unload current and load new
    pub async fn swap_model(
        &self,
        from: &ModelId,
        to_config: ModelConfig,
    ) -> Result<ModelId, LLMError> {
        info!(from = %from, "Swapping models");
        
        // Pre-load new model weights to CPU
        self.preload_to_cpu(from).await?;
        
        // Unload old model
        self.unload_model(from).await?;
        
        // Load new model
        self.load_model(to_config).await
    }
    
    /// Preload model to CPU cache
    async fn preload_to_cpu(&self, model_id: &ModelId) -> Result<(), LLMError> {
        debug!(model_id = %model_id, "Preloading model to CPU cache");
        
        // In a real implementation, this would copy weights from GPU to CPU
        // For now, just mark it
        
        let cache_entry = ModelCacheEntry {
            model_id: model_id.clone(),
            cpu_weights: Vec::new(), // Would contain actual weights
            cached_at: Instant::now(),
        };
        
        self.cpu_cache.write().await.insert(model_id.clone(), cache_entry);
        
        Ok(())
    }
    
    /// Load model from CPU cache
    async fn load_from_cpu_cache(&self, model_id: &ModelId) -> Result<ModelId, LLMError> {
        let cache = self.cpu_cache.read().await;
        
        if let Some(entry) = cache.get(model_id) {
            debug!(model_id = %model_id, "Loading model from CPU cache");
            
            // In a real implementation, this would load from CPU to GPU
            // which is faster than loading from disk
            
            Ok(entry.model_id.clone())
        } else {
            Err(LLMError::ModelNotFound(model_id.to_string()))
        }
    }
    
    /// Create inference engine for model
    async fn create_engine(&self, config: &ModelConfig) -> Result<Arc<dyn LLMEngine>, LLMError> {
        // In a real implementation, this would create the appropriate engine
        // based on the provider (vLLM, Ollama, etc.)
        
        match self.config.provider.parse::<super::LLMProvider>() {
            Ok(super::LLMProvider::VLLM) => {
                // Create vLLM engine
                // For now, return a placeholder
                Err(LLMError::ModelLoadFailed("vLLM not yet implemented".to_string()))
            }
            Ok(super::LLMProvider::Ollama) => {
                // Create Ollama engine
                Err(LLMError::ModelLoadFailed("Ollama not yet implemented".to_string()))
            }
            Err(e) => Err(LLMError::ModelLoadFailed(e.to_string())),
        }
    }
    
    /// Estimate memory required for model
    async fn estimate_memory(&self, config: &ModelConfig) -> u64 {
        // Rough estimation based on model size and quantization
        let base_size = 30_000_000_000u64; // 30GB for 32B model at FP16
        
        let multiplier = match config.quantization {
            QuantizationLevel::None => 2.0,
            QuantizationLevel::FP8 => 1.0,
            QuantizationLevel::Int8 => 0.5,
            QuantizationLevel::Int4 => 0.25,
        };
        
        (base_size as f32 * multiplier) as u64
    }
    
    /// Update last used time for a model
    pub async fn touch_model(&self, model_id: &ModelId) {
        let models = self.models.read().await;
        if let Some(instance) = models.get(model_id) {
            *instance.last_used.write().await = Instant::now();
            instance.use_count.fetch_add(1, Ordering::Relaxed);
        }
    }
    
    /// Get model statistics
    pub async fn get_stats(&self) -> ModelStats {
        let models = self.models.read().await;
        
        ModelStats {
            loaded_models: models.len(),
            total_memory_used: models
                .values()
                .map(|m| self.estimate_memory(&m.config).await)
                .sum(),
            cached_models: self.cpu_cache.read().await.len(),
        }
    }
    
    /// Cleanup old CPU cache entries
    pub async fn cleanup_cpu_cache(&self, max_age: Duration) -> usize {
        let mut cache = self.cpu_cache.write().await;
        let now = Instant::now();
        
        let to_remove: Vec<_> = cache
            .iter()
            .filter(|(_, entry)| now.duration_since(entry.cached_at) > max_age)
            .map(|(id, _)| id.clone())
            .collect();
        
        for id in &to_remove {
            cache.remove(id);
        }
        
        to_remove.len()
    }
}

/// Model statistics
#[derive(Debug, Clone)]
pub struct ModelStats {
    pub loaded_models: usize,
    pub total_memory_used: u64,
    pub cached_models: usize,
}
