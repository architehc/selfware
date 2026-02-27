//! Resource quotas and adaptive limits

use super::ResourceRequest;
use crate::config::ResourceQuotas;
use crate::error::ResourceError;
use crate::resource::ResourcePressure;
use tokio::sync::RwLock;

/// Adaptive quotas that adjust based on system pressure
pub struct AdaptiveQuotas {
    base: ResourceQuotas,
    current: RwLock<ResourceQuotas>,
}

impl AdaptiveQuotas {
    /// Create new adaptive quotas
    pub fn new(base: ResourceQuotas) -> Self {
        Self {
            base: base.clone(),
            current: RwLock::new(base),
        }
    }
    
    /// Adjust quotas based on resource pressure
    pub async fn adjust_for_pressure(&self, pressure: ResourcePressure) {
        let mut current = self.current.write().await;
        
        match pressure {
            ResourcePressure::None => {
                // Reset to base quotas
                *current = self.base.clone();
            }
            ResourcePressure::Low => {
                // Slight reduction
                current.max_concurrent_requests = self.base.max_concurrent_requests.saturating_sub(1);
            }
            ResourcePressure::Medium => {
                // Moderate reduction
                current.max_concurrent_requests = self.base.max_concurrent_requests / 2;
                current.max_context_tokens = self.base.max_context_tokens / 2;
            }
            ResourcePressure::High => {
                // Significant reduction
                current.max_concurrent_requests = 1;
                current.max_context_tokens = self.base.max_context_tokens / 4;
                current.max_queued_tasks = self.base.max_queued_tasks / 2;
            }
            ResourcePressure::Critical => {
                // Emergency mode
                current.max_concurrent_requests = 1;
                current.max_context_tokens = 8192;
                current.max_queued_tasks = 10;
                current.max_gpu_memory_per_model = self.base.max_gpu_memory_per_model / 2;
            }
        }
    }
    
    /// Check if a resource request is within quotas
    pub async fn check(&self, request: &ResourceRequest) -> Result<(), ResourceError> {
        let quotas = self.current.read().await;
        
        if request.gpu_memory_bytes > quotas.max_gpu_memory_per_model {
            return Err(ResourceError::QuotaExceeded {
                resource: "gpu_memory_per_model".to_string(),
                used: request.gpu_memory_bytes,
                limit: quotas.max_gpu_memory_per_model,
            });
        }
        
        if request.system_memory_bytes > quotas.max_context_tokens as u64 * 100 {
            return Err(ResourceError::QuotaExceeded {
                resource: "system_memory".to_string(),
                used: request.system_memory_bytes,
                limit: quotas.max_context_tokens as u64 * 100,
            });
        }
        
        Ok(())
    }
    
    /// Get current quotas
    pub async fn current(&self) -> ResourceQuotas {
        self.current.read().await.clone()
    }
    
    /// Get base quotas
    pub fn base(&self) -> &ResourceQuotas {
        &self.base
    }
}

/// Resource limit tracker
pub struct ResourceLimitTracker {
    quotas: ResourceQuotas,
    current_gpu_memory: RwLock<u64>,
    current_concurrent_requests: RwLock<usize>,
    current_queued_tasks: RwLock<usize>,
}

impl ResourceLimitTracker {
    /// Create a new resource limit tracker
    pub fn new(quotas: ResourceQuotas) -> Self {
        Self {
            quotas,
            current_gpu_memory: RwLock::new(0),
            current_concurrent_requests: RwLock::new(0),
            current_queued_tasks: RwLock::new(0),
        }
    }
    
    /// Try to allocate GPU memory
    pub async fn allocate_gpu_memory(&self, bytes: u64) -> Result<GPUAllocationGuard, ResourceError> {
        let mut current = self.current_gpu_memory.write().await;
        let new_total = *current + bytes;
        
        if new_total > self.quotas.max_gpu_memory_per_model {
            return Err(ResourceError::QuotaExceeded {
                resource: "gpu_memory".to_string(),
                used: new_total,
                limit: self.quotas.max_gpu_memory_per_model,
            });
        }
        
        *current = new_total;
        
        Ok(GPUAllocationGuard {
            tracker: self,
            bytes,
        })
    }
    
    /// Try to start a concurrent request
    pub async fn start_request(&self) -> Result<RequestGuard, ResourceError> {
        let mut current = self.current_concurrent_requests.write().await;
        
        if *current >= self.quotas.max_concurrent_requests {
            return Err(ResourceError::QuotaExceeded {
                resource: "concurrent_requests".to_string(),
                used: *current as u64,
                limit: self.quotas.max_concurrent_requests as u64,
            });
        }
        
        *current += 1;
        
        Ok(RequestGuard { tracker: self })
    }
    
    /// Try to queue a task
    pub async fn queue_task(&self) -> Result<TaskGuard, ResourceError> {
        let mut current = self.current_queued_tasks.write().await;
        
        if *current >= self.quotas.max_queued_tasks {
            return Err(ResourceError::QuotaExceeded {
                resource: "queued_tasks".to_string(),
                used: *current as u64,
                limit: self.quotas.max_queued_tasks as u64,
            });
        }
        
        *current += 1;
        
        Ok(TaskGuard { tracker: self })
    }
    
    /// Release GPU memory
    async fn release_gpu_memory(&self, bytes: u64) {
        let mut current = self.current_gpu_memory.write().await;
        *current = current.saturating_sub(bytes);
    }
    
    /// Release request slot
    async fn release_request(&self) {
        let mut current = self.current_concurrent_requests.write().await;
        *current = current.saturating_sub(1);
    }
    
    /// Release task slot
    async fn release_task(&self) {
        let mut current = self.current_queued_tasks.write().await;
        *current = current.saturating_sub(1);
    }
    
    /// Get current usage
    pub async fn usage(&self) -> ResourceUsage {
        ResourceUsage {
            gpu_memory: *self.current_gpu_memory.read().await,
            concurrent_requests: *self.current_concurrent_requests.read().await,
            queued_tasks: *self.current_queued_tasks.read().await,
        }
    }
}

/// Current resource usage
#[derive(Debug, Clone)]
pub struct ResourceUsage {
    pub gpu_memory: u64,
    pub concurrent_requests: usize,
    pub queued_tasks: usize,
}

/// RAII guard for GPU memory allocation
pub struct GPUAllocationGuard<'a> {
    tracker: &'a ResourceLimitTracker,
    bytes: u64,
}

impl<'a> Drop for GPUAllocationGuard<'a> {
    fn drop(&mut self) {
        let tracker = self.tracker;
        let bytes = self.bytes;
        tokio::spawn(async move {
            tracker.release_gpu_memory(bytes).await;
        });
    }
}

/// RAII guard for concurrent request
pub struct RequestGuard<'a> {
    tracker: &'a ResourceLimitTracker,
}

impl<'a> Drop for RequestGuard<'a> {
    fn drop(&mut self) {
        let tracker = self.tracker;
        tokio::spawn(async move {
            tracker.release_request().await;
        });
    }
}

/// RAII guard for queued task
pub struct TaskGuard<'a> {
    tracker: &'a ResourceLimitTracker,
}

impl<'a> Drop for TaskGuard<'a> {
    fn drop(&mut self) {
        let tracker = self.tracker;
        tokio::spawn(async move {
            tracker.release_task().await;
        });
    }
}
