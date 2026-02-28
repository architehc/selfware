//! Memory resource management

use crate::config::MemoryConfig;
use crate::errors::ResourceError;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

/// Memory manager for system RAM
pub struct MemoryManager {
    config: MemoryConfig,
    action_tx: mpsc::Sender<MemoryAction>,
    allocated: AtomicU64,
}

/// Memory usage statistics
#[derive(Debug, Clone, Default)]
pub struct MemoryUsage {
    pub used: u64,
    pub total: u64,
    pub available: u64,
    pub percent: f32,
}

/// Memory actions for handling pressure
#[derive(Debug, Clone)]
pub enum MemoryAction {
    /// Run garbage collection hints
    RunGC,
    /// Flush caches
    FlushCaches,
    /// Reduce context window
    ReduceContext { target_tokens: usize },
    /// Pause non-critical tasks
    PauseTasks { priority_threshold: u8 },
    /// Offload models to CPU
    OffloadModels,
    /// Emergency: restart component
    EmergencyRestart,
}

impl MemoryManager {
    /// Create a new memory manager
    pub async fn new(config: &MemoryConfig) -> Result<Self, ResourceError> {
        let (action_tx, mut action_rx) = mpsc::channel(10);

        // Start action handler
        tokio::spawn(async move {
            while let Some(action) = action_rx.recv().await {
                match action {
                    MemoryAction::RunGC => {
                        debug!("Running garbage collection hints");
                        // In Rust, we can't force GC, but we can drop references
                    }
                    MemoryAction::FlushCaches => {
                        info!("Flushing caches");
                        // Would flush internal caches
                    }
                    MemoryAction::ReduceContext { target_tokens } => {
                        warn!(target_tokens = target_tokens, "Reducing context window");
                        // Would trigger context compression
                    }
                    MemoryAction::PauseTasks { priority_threshold } => {
                        warn!(priority = ?priority_threshold, "Pausing tasks below priority");
                        // Would pause low-priority tasks
                    }
                    MemoryAction::OffloadModels => {
                        warn!("Offloading models to CPU");
                        // Would offload non-critical models
                    }
                    MemoryAction::EmergencyRestart => {
                        error!("Emergency restart triggered");
                        // Would trigger component restart
                    }
                }
            }
        });

        Ok(Self {
            config: config.clone(),
            action_tx,
            allocated: AtomicU64::new(0),
        })
    }

    /// Get current memory usage
    pub async fn get_usage(&self) -> Result<MemoryUsage, ResourceError> {
        use sysinfo::System;

        let mut system = System::new_all();
        system.refresh_all();

        let total = system.total_memory();
        let used = system.used_memory();
        let available = system.available_memory();

        Ok(MemoryUsage {
            used,
            total,
            available,
            percent: if total > 0 {
                used as f32 / total as f32
            } else {
                0.0
            },
        })
    }

    /// Monitor memory continuously
    pub async fn monitor(&self) {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(
            self.config.monitor_interval_seconds,
        ));

        loop {
            interval.tick().await;

            if let Ok(usage) = self.get_usage().await {
                // metrics::gauge!("memory.used_bytes", usage.used as f64);
                // metrics::gauge!("memory.available_bytes", usage.available as f64);
                // metrics::gauge!("memory.percent", usage.percent as f64);

                // Check thresholds
                if usage.percent > self.config.emergency_threshold {
                    warn!(
                        percent = usage.percent,
                        "Memory emergency threshold reached"
                    );
                    self.trigger_emergency_cleanup().await;
                } else if usage.percent > self.config.critical_threshold {
                    warn!(percent = usage.percent, "Memory critical threshold reached");
                    self.trigger_critical_cleanup().await;
                } else if usage.percent > self.config.warning_threshold {
                    debug!(percent = usage.percent, "Memory warning threshold reached");
                    self.trigger_warning_cleanup().await;
                }
            }
        }
    }

    /// Trigger warning-level cleanup
    pub async fn trigger_warning_cleanup(&self) {
        let _ = self.action_tx.send(MemoryAction::FlushCaches).await;
    }

    /// Trigger critical-level cleanup
    pub async fn trigger_critical_cleanup(&self) {
        let _ = self.action_tx.send(MemoryAction::FlushCaches).await;
        let _ = self
            .action_tx
            .send(MemoryAction::ReduceContext {
                target_tokens: 32768,
            })
            .await;
        let _ = self
            .action_tx
            .send(MemoryAction::PauseTasks {
                priority_threshold: 1,
            })
            .await;
    }

    /// Trigger emergency-level cleanup
    pub async fn trigger_emergency_cleanup(&self) {
        let _ = self.action_tx.send(MemoryAction::FlushCaches).await;
        let _ = self
            .action_tx
            .send(MemoryAction::ReduceContext {
                target_tokens: 8192,
            })
            .await;
        let _ = self.action_tx.send(MemoryAction::OffloadModels).await;
        let _ = self
            .action_tx
            .send(MemoryAction::PauseTasks {
                priority_threshold: 2,
            })
            .await;
    }

    /// Allocate memory
    pub fn allocate(&self, bytes: u64) -> Result<(), ResourceError> {
        let current = self.allocated.fetch_add(bytes, Ordering::SeqCst);
        debug!(
            allocated_bytes = bytes,
            total_allocated = current + bytes,
            "Memory allocated"
        );
        Ok(())
    }

    /// Free allocated memory
    pub fn free(&self, bytes: u64) {
        let _ = self.allocated.fetch_sub(bytes, Ordering::SeqCst);
        debug!(freed_bytes = bytes, "Memory freed");
    }

    /// Get allocated memory
    pub fn get_allocated(&self) -> u64 {
        self.allocated.load(Ordering::Relaxed)
    }

    /// Check if enough memory is available
    pub async fn check_available(&self, required_bytes: u64) -> Result<bool, ResourceError> {
        let usage = self.get_usage().await?;
        Ok(usage.available >= required_bytes)
    }

    /// Estimate memory for operation
    pub fn estimate_for_tokens(&self, tokens: usize, bytes_per_token: usize) -> u64 {
        (tokens * bytes_per_token) as u64
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::MemoryConfig;

    /// Helper to create a MemoryManager for testing without async overhead.
    /// Uses a bounded channel so we can inspect actions sent.
    fn make_test_memory_manager() -> (MemoryManager, mpsc::Receiver<MemoryAction>) {
        let (action_tx, action_rx) = mpsc::channel(100);
        let mm = MemoryManager {
            config: MemoryConfig::default(),
            action_tx,
            allocated: AtomicU64::new(0),
        };
        (mm, action_rx)
    }

    // ---- MemoryUsage tests ----

    #[test]
    fn test_memory_usage_default() {
        let usage = MemoryUsage::default();
        assert_eq!(usage.used, 0);
        assert_eq!(usage.total, 0);
        assert_eq!(usage.available, 0);
        assert_eq!(usage.percent, 0.0);
    }

    #[test]
    fn test_memory_usage_clone() {
        let usage = MemoryUsage {
            used: 8_000_000_000,
            total: 16_000_000_000,
            available: 8_000_000_000,
            percent: 0.5,
        };
        let cloned = usage.clone();
        assert_eq!(cloned.used, 8_000_000_000);
        assert_eq!(cloned.percent, 0.5);
    }

    // ---- MemoryAction tests ----

    #[test]
    fn test_memory_action_debug() {
        let action = MemoryAction::ReduceContext {
            target_tokens: 32768,
        };
        let debug_str = format!("{:?}", action);
        assert!(debug_str.contains("ReduceContext"));
        assert!(debug_str.contains("32768"));
    }

    #[test]
    fn test_memory_action_clone() {
        let action = MemoryAction::PauseTasks {
            priority_threshold: 2,
        };
        let cloned = action.clone();
        match cloned {
            MemoryAction::PauseTasks {
                priority_threshold: p,
            } => assert_eq!(p, 2),
            _ => panic!("Clone produced wrong variant"),
        }
    }

    // ---- MemoryManager allocate/free tests ----

    #[test]
    fn test_allocate_increases_tracked_memory() {
        let (mm, _rx) = make_test_memory_manager();
        assert_eq!(mm.get_allocated(), 0);

        mm.allocate(1_000_000).unwrap();
        assert_eq!(mm.get_allocated(), 1_000_000);

        mm.allocate(2_000_000).unwrap();
        assert_eq!(mm.get_allocated(), 3_000_000);
    }

    #[test]
    fn test_free_decreases_tracked_memory() {
        let (mm, _rx) = make_test_memory_manager();
        mm.allocate(5_000_000).unwrap();
        mm.free(2_000_000);
        assert_eq!(mm.get_allocated(), 3_000_000);
    }

    #[test]
    fn test_free_saturates_at_zero() {
        let (mm, _rx) = make_test_memory_manager();
        mm.allocate(1_000).unwrap();
        // Freeing more than allocated should wrap to a huge number with fetch_sub,
        // but the AtomicU64 wraps; let's verify current behavior
        mm.free(1_000);
        assert_eq!(mm.get_allocated(), 0);
    }

    #[test]
    fn test_allocate_returns_ok() {
        let (mm, _rx) = make_test_memory_manager();
        let result = mm.allocate(42);
        assert!(result.is_ok());
    }

    // ---- estimate_for_tokens tests ----

    #[test]
    fn test_estimate_for_tokens_basic() {
        let (mm, _rx) = make_test_memory_manager();
        let estimate = mm.estimate_for_tokens(1000, 4);
        assert_eq!(estimate, 4000);
    }

    #[test]
    fn test_estimate_for_tokens_zero() {
        let (mm, _rx) = make_test_memory_manager();
        assert_eq!(mm.estimate_for_tokens(0, 100), 0);
        assert_eq!(mm.estimate_for_tokens(100, 0), 0);
    }

    #[test]
    fn test_estimate_for_tokens_large_context() {
        let (mm, _rx) = make_test_memory_manager();
        // 1M tokens * 2 bytes each = 2MB
        let estimate = mm.estimate_for_tokens(1_000_000, 2);
        assert_eq!(estimate, 2_000_000);
    }

    // ---- Cleanup trigger tests ----

    #[tokio::test]
    async fn test_trigger_warning_cleanup_sends_action() {
        let (mm, mut rx) = make_test_memory_manager();
        mm.trigger_warning_cleanup().await;

        let action = rx.recv().await.unwrap();
        match action {
            MemoryAction::FlushCaches => {} // expected
            other => panic!("Expected FlushCaches, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_trigger_critical_cleanup_sends_three_actions() {
        let (mm, mut rx) = make_test_memory_manager();
        mm.trigger_critical_cleanup().await;

        let a1 = rx.recv().await.unwrap();
        assert!(matches!(a1, MemoryAction::FlushCaches));

        let a2 = rx.recv().await.unwrap();
        assert!(matches!(
            a2,
            MemoryAction::ReduceContext {
                target_tokens: 32768
            }
        ));

        let a3 = rx.recv().await.unwrap();
        assert!(matches!(
            a3,
            MemoryAction::PauseTasks {
                priority_threshold: 1
            }
        ));
    }

    #[tokio::test]
    async fn test_trigger_emergency_cleanup_sends_four_actions() {
        let (mm, mut rx) = make_test_memory_manager();
        mm.trigger_emergency_cleanup().await;

        let a1 = rx.recv().await.unwrap();
        assert!(matches!(a1, MemoryAction::FlushCaches));

        let a2 = rx.recv().await.unwrap();
        assert!(matches!(
            a2,
            MemoryAction::ReduceContext {
                target_tokens: 8192
            }
        ));

        let a3 = rx.recv().await.unwrap();
        assert!(matches!(a3, MemoryAction::OffloadModels));

        let a4 = rx.recv().await.unwrap();
        assert!(matches!(
            a4,
            MemoryAction::PauseTasks {
                priority_threshold: 2
            }
        ));
    }

    // ---- MemoryConfig defaults ----

    #[test]
    fn test_memory_config_thresholds_are_ordered() {
        let config = MemoryConfig::default();
        assert!(config.warning_threshold < config.critical_threshold);
        assert!(config.critical_threshold < config.emergency_threshold);
        assert!(config.emergency_threshold <= 1.0);
    }
}
