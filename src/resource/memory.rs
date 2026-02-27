//! Memory resource management

use crate::config::MemoryConfig;
use crate::errors::ResourceError;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{debug, info, warn, error};

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
        let mut interval = tokio::time::interval(
            std::time::Duration::from_secs(self.config.monitor_interval_seconds)
        );
        
        loop {
            interval.tick().await;
            
            if let Ok(usage) = self.get_usage().await {
                // metrics::gauge!("memory.used_bytes", usage.used as f64);
                // metrics::gauge!("memory.available_bytes", usage.available as f64);
                // metrics::gauge!("memory.percent", usage.percent as f64);
                
                // Check thresholds
                if usage.percent > self.config.emergency_threshold {
                    warn!(percent = usage.percent, "Memory emergency threshold reached");
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
        let _ = self.action_tx.send(MemoryAction::ReduceContext { target_tokens: 32768 }).await;
        let _ = self.action_tx.send(MemoryAction::PauseTasks { 
            priority_threshold: 1 
        }).await;
    }
    
    /// Trigger emergency-level cleanup
    pub async fn trigger_emergency_cleanup(&self) {
        let _ = self.action_tx.send(MemoryAction::FlushCaches).await;
        let _ = self.action_tx.send(MemoryAction::ReduceContext { target_tokens: 8192 }).await;
        let _ = self.action_tx.send(MemoryAction::OffloadModels).await;
        let _ = self.action_tx.send(MemoryAction::PauseTasks { 
            priority_threshold: 2 
        }).await;
    }
    
    /// Allocate memory
    pub fn allocate(&self, bytes: u64) -> Result<(), ResourceError> {
        let current = self.allocated.fetch_add(bytes, Ordering::SeqCst);
        debug!(allocated_bytes = bytes, total_allocated = current + bytes, "Memory allocated");
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
