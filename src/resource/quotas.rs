//! Resource quotas and adaptive limits

use super::ResourceRequest;
use crate::config::ResourceQuotas;
use crate::errors::ResourceError;
use crate::resource::ResourcePressure;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
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
                current.max_concurrent_requests =
                    self.base.max_concurrent_requests.saturating_sub(1);
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
    current_gpu_memory: AtomicU64,
    current_concurrent_requests: AtomicUsize,
    current_queued_tasks: AtomicUsize,
}

impl ResourceLimitTracker {
    /// Create a new resource limit tracker
    pub fn new(quotas: ResourceQuotas) -> Self {
        Self {
            quotas,
            current_gpu_memory: AtomicU64::new(0),
            current_concurrent_requests: AtomicUsize::new(0),
            current_queued_tasks: AtomicUsize::new(0),
        }
    }

    /// Try to allocate GPU memory
    pub fn allocate_gpu_memory(&self, bytes: u64) -> Result<GPUAllocationGuard<'_>, ResourceError> {
        let mut current = self.current_gpu_memory.load(Ordering::SeqCst);
        loop {
            let new_total = current + bytes;
            if new_total > self.quotas.max_gpu_memory_per_model {
                return Err(ResourceError::QuotaExceeded {
                    resource: "gpu_memory".to_string(),
                    used: new_total,
                    limit: self.quotas.max_gpu_memory_per_model,
                });
            }
            match self.current_gpu_memory.compare_exchange_weak(
                current,
                new_total,
                Ordering::SeqCst,
                Ordering::SeqCst,
            ) {
                Ok(_) => break,
                Err(c) => current = c,
            }
        }

        Ok(GPUAllocationGuard {
            tracker: self,
            bytes,
        })
    }

    /// Try to start a concurrent request
    pub fn start_request(&self) -> Result<RequestGuard<'_>, ResourceError> {
        let mut current = self.current_concurrent_requests.load(Ordering::SeqCst);
        loop {
            if current >= self.quotas.max_concurrent_requests {
                return Err(ResourceError::QuotaExceeded {
                    resource: "concurrent_requests".to_string(),
                    used: current as u64,
                    limit: self.quotas.max_concurrent_requests as u64,
                });
            }
            match self.current_concurrent_requests.compare_exchange_weak(
                current,
                current + 1,
                Ordering::SeqCst,
                Ordering::SeqCst,
            ) {
                Ok(_) => break,
                Err(c) => current = c,
            }
        }

        Ok(RequestGuard { tracker: self })
    }

    /// Try to queue a task
    pub fn queue_task(&self) -> Result<TaskGuard<'_>, ResourceError> {
        let mut current = self.current_queued_tasks.load(Ordering::SeqCst);
        loop {
            if current >= self.quotas.max_queued_tasks {
                return Err(ResourceError::QuotaExceeded {
                    resource: "queued_tasks".to_string(),
                    used: current as u64,
                    limit: self.quotas.max_queued_tasks as u64,
                });
            }
            match self.current_queued_tasks.compare_exchange_weak(
                current,
                current + 1,
                Ordering::SeqCst,
                Ordering::SeqCst,
            ) {
                Ok(_) => break,
                Err(c) => current = c,
            }
        }

        Ok(TaskGuard { tracker: self })
    }

    /// Release GPU memory
    fn release_gpu_memory(&self, bytes: u64) {
        self.current_gpu_memory.fetch_sub(bytes, Ordering::SeqCst);
    }

    /// Release request slot
    fn release_request(&self) {
        self.current_concurrent_requests
            .fetch_sub(1, Ordering::SeqCst);
    }

    /// Release task slot
    fn release_task(&self) {
        self.current_queued_tasks.fetch_sub(1, Ordering::SeqCst);
    }

    /// Get current usage
    pub fn usage(&self) -> ResourceUsage {
        ResourceUsage {
            gpu_memory: self.current_gpu_memory.load(Ordering::SeqCst),
            concurrent_requests: self.current_concurrent_requests.load(Ordering::SeqCst),
            queued_tasks: self.current_queued_tasks.load(Ordering::SeqCst),
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
        self.tracker.release_gpu_memory(self.bytes);
    }
}

/// RAII guard for concurrent request
pub struct RequestGuard<'a> {
    tracker: &'a ResourceLimitTracker,
}

impl<'a> Drop for RequestGuard<'a> {
    fn drop(&mut self) {
        self.tracker.release_request();
    }
}

/// RAII guard for queued task
pub struct TaskGuard<'a> {
    tracker: &'a ResourceLimitTracker,
}

impl<'a> Drop for TaskGuard<'a> {
    fn drop(&mut self) {
        self.tracker.release_task();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ResourceQuotas;
    use std::time::Duration;

    fn default_quotas() -> ResourceQuotas {
        ResourceQuotas::default()
    }

    fn small_quotas() -> ResourceQuotas {
        ResourceQuotas {
            max_gpu_memory_per_model: 1_000_000,
            max_concurrent_requests: 2,
            max_context_tokens: 1000,
            max_queued_tasks: 5,
            max_checkpoint_size: 500_000,
        }
    }

    // ---- AdaptiveQuotas tests ----

    #[tokio::test]
    async fn test_adaptive_quotas_new_stores_base() {
        let base = small_quotas();
        let aq = AdaptiveQuotas::new(base.clone());
        assert_eq!(aq.base().max_concurrent_requests, 2);
        let current = aq.current().await;
        assert_eq!(current.max_concurrent_requests, 2);
    }

    #[tokio::test]
    async fn test_adjust_for_pressure_none_resets_to_base() {
        let base = small_quotas();
        let aq = AdaptiveQuotas::new(base.clone());
        // First apply high pressure
        aq.adjust_for_pressure(ResourcePressure::High).await;
        // Then reset
        aq.adjust_for_pressure(ResourcePressure::None).await;
        let current = aq.current().await;
        assert_eq!(current.max_concurrent_requests, base.max_concurrent_requests);
        assert_eq!(current.max_context_tokens, base.max_context_tokens);
    }

    #[tokio::test]
    async fn test_adjust_for_pressure_low_reduces_requests_by_one() {
        let base = default_quotas(); // max_concurrent_requests = 4
        let aq = AdaptiveQuotas::new(base.clone());
        aq.adjust_for_pressure(ResourcePressure::Low).await;
        let current = aq.current().await;
        assert_eq!(
            current.max_concurrent_requests,
            base.max_concurrent_requests - 1
        );
    }

    #[tokio::test]
    async fn test_adjust_for_pressure_medium_halves_requests_and_tokens() {
        let base = default_quotas();
        let aq = AdaptiveQuotas::new(base.clone());
        aq.adjust_for_pressure(ResourcePressure::Medium).await;
        let current = aq.current().await;
        assert_eq!(
            current.max_concurrent_requests,
            base.max_concurrent_requests / 2
        );
        assert_eq!(current.max_context_tokens, base.max_context_tokens / 2);
    }

    #[tokio::test]
    async fn test_adjust_for_pressure_high_sets_single_request() {
        let base = default_quotas();
        let aq = AdaptiveQuotas::new(base.clone());
        aq.adjust_for_pressure(ResourcePressure::High).await;
        let current = aq.current().await;
        assert_eq!(current.max_concurrent_requests, 1);
        assert_eq!(current.max_context_tokens, base.max_context_tokens / 4);
        assert_eq!(current.max_queued_tasks, base.max_queued_tasks / 2);
    }

    #[tokio::test]
    async fn test_adjust_for_pressure_critical_emergency_mode() {
        let base = default_quotas();
        let aq = AdaptiveQuotas::new(base.clone());
        aq.adjust_for_pressure(ResourcePressure::Critical).await;
        let current = aq.current().await;
        assert_eq!(current.max_concurrent_requests, 1);
        assert_eq!(current.max_context_tokens, 8192);
        assert_eq!(current.max_queued_tasks, 10);
        assert_eq!(
            current.max_gpu_memory_per_model,
            base.max_gpu_memory_per_model / 2
        );
    }

    #[tokio::test]
    async fn test_quota_check_passes_within_limits() {
        let aq = AdaptiveQuotas::new(default_quotas());
        let request = ResourceRequest {
            gpu_memory_bytes: 1_000_000_000, // 1GB, well under 20GB limit
            system_memory_bytes: 1_000_000,
            disk_bytes: 0,
            duration_estimate: Duration::from_secs(10),
        };
        assert!(aq.check(&request).await.is_ok());
    }

    #[tokio::test]
    async fn test_quota_check_fails_gpu_memory_exceeded() {
        let aq = AdaptiveQuotas::new(small_quotas()); // max_gpu_memory_per_model = 1_000_000
        let request = ResourceRequest {
            gpu_memory_bytes: 2_000_000, // exceeds 1_000_000
            system_memory_bytes: 0,
            disk_bytes: 0,
            duration_estimate: Duration::from_secs(1),
        };
        let result = aq.check(&request).await;
        assert!(result.is_err());
        match result {
            Err(ResourceError::QuotaExceeded { resource, .. }) => {
                assert_eq!(resource, "gpu_memory_per_model");
            }
            _ => panic!("Expected QuotaExceeded for gpu_memory_per_model"),
        }
    }

    #[tokio::test]
    async fn test_quota_check_fails_system_memory_exceeded() {
        let aq = AdaptiveQuotas::new(small_quotas()); // max_context_tokens = 1000
        // system_memory limit is max_context_tokens * 100 = 100_000
        let request = ResourceRequest {
            gpu_memory_bytes: 0,
            system_memory_bytes: 200_000, // exceeds 100_000
            disk_bytes: 0,
            duration_estimate: Duration::from_secs(1),
        };
        let result = aq.check(&request).await;
        assert!(result.is_err());
        match result {
            Err(ResourceError::QuotaExceeded { resource, .. }) => {
                assert_eq!(resource, "system_memory");
            }
            _ => panic!("Expected QuotaExceeded for system_memory"),
        }
    }

    // ---- ResourceLimitTracker tests ----

    #[test]
    fn test_tracker_gpu_memory_allocate_and_release() {
        let tracker = ResourceLimitTracker::new(small_quotas());
        {
            let guard = tracker.allocate_gpu_memory(500_000).unwrap();
            assert_eq!(tracker.usage().gpu_memory, 500_000);
            drop(guard); // RAII release
        }
        assert_eq!(tracker.usage().gpu_memory, 0);
    }

    #[test]
    fn test_tracker_gpu_memory_exceeds_quota() {
        let tracker = ResourceLimitTracker::new(small_quotas()); // max = 1_000_000
        let result = tracker.allocate_gpu_memory(2_000_000);
        assert!(result.is_err());
    }

    #[test]
    fn test_tracker_concurrent_requests() {
        let tracker = ResourceLimitTracker::new(small_quotas()); // max = 2
        let _g1 = tracker.start_request().unwrap();
        let _g2 = tracker.start_request().unwrap();
        // Third should fail
        let result = tracker.start_request();
        assert!(result.is_err());
    }

    #[test]
    fn test_tracker_request_guard_releases_on_drop() {
        let tracker = ResourceLimitTracker::new(small_quotas()); // max = 2
        {
            let _g1 = tracker.start_request().unwrap();
            let _g2 = tracker.start_request().unwrap();
            assert_eq!(tracker.usage().concurrent_requests, 2);
        }
        // Both guards dropped
        assert_eq!(tracker.usage().concurrent_requests, 0);
        // Should be able to start again
        let _g = tracker.start_request().unwrap();
        assert_eq!(tracker.usage().concurrent_requests, 1);
    }

    #[test]
    fn test_tracker_queue_task() {
        let tracker = ResourceLimitTracker::new(small_quotas()); // max_queued = 5
        let mut guards = Vec::new();
        for _ in 0..5 {
            guards.push(tracker.queue_task().unwrap());
        }
        // 6th should fail
        let result = tracker.queue_task();
        assert!(result.is_err());
        assert_eq!(tracker.usage().queued_tasks, 5);
    }

    #[test]
    fn test_tracker_task_guard_releases_on_drop() {
        let tracker = ResourceLimitTracker::new(small_quotas());
        {
            let _g = tracker.queue_task().unwrap();
            assert_eq!(tracker.usage().queued_tasks, 1);
        }
        assert_eq!(tracker.usage().queued_tasks, 0);
    }

    #[test]
    fn test_tracker_usage_initial_state() {
        let tracker = ResourceLimitTracker::new(default_quotas());
        let usage = tracker.usage();
        assert_eq!(usage.gpu_memory, 0);
        assert_eq!(usage.concurrent_requests, 0);
        assert_eq!(usage.queued_tasks, 0);
    }

    // ---- ResourceUsage (quotas) tests ----

    #[test]
    fn test_resource_usage_clone() {
        let usage = ResourceUsage {
            gpu_memory: 42,
            concurrent_requests: 3,
            queued_tasks: 7,
        };
        let cloned = usage.clone();
        assert_eq!(cloned.gpu_memory, 42);
        assert_eq!(cloned.concurrent_requests, 3);
        assert_eq!(cloned.queued_tasks, 7);
    }
}
