//! Parallel execution system
//!
//! This module provides tools for executing tool calls in parallel,
//! detecting conflicts, and managing dependencies between operations.

pub mod conflicts;
pub mod executor;
pub mod types;

pub use conflicts::{Conflict, ConflictDetector, ConflictResolver, ConflictType};
pub use executor::ParallelExecutor;
pub use types::{
    DependencyGraph, NodeStatus, ParallelConfig, ParallelResult,
};

use std::sync::Arc;
use tokio::sync::RwLock;



/// Manages parallel execution of tool calls
pub struct ParallelManager {
    executor: Arc<ParallelExecutor>,
    config: Arc<RwLock<ParallelConfig>>,
}

impl ParallelManager {
    /// Create a new parallel manager with default configuration
    pub fn new() -> Self {
        let config = Arc::new(RwLock::new(ParallelConfig::default()));
        let executor = Arc::new(ParallelExecutor::new(ParallelConfig::default()));

        Self { executor, config }
    }

    /// Create with custom configuration
    pub fn with_config(config: ParallelConfig) -> Self {
        let cfg = Arc::new(RwLock::new(config.clone()));
        let executor = Arc::new(ParallelExecutor::new(config));

        Self {
            executor,
            config: cfg,
        }
    }

    /// Get the underlying executor
    pub fn executor(&self) -> &ParallelExecutor {
        &self.executor
    }

    /// Get the configuration
    pub fn config(&self) -> Arc<RwLock<ParallelConfig>> {
        self.config.clone()
    }

    /// Enable parallel execution
    pub async fn enable(&self) {
        self.executor.enable().await;
    }

    /// Disable parallel execution
    pub async fn disable(&self) {
        self.executor.disable().await;
    }

    /// Set maximum concurrency level
    pub async fn set_max_concurrency(&self, max: usize) {
        self.executor.set_max_concurrency(max).await;
    }
}

impl Default for ParallelManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Re-export commonly used types
pub mod prelude {
    pub use super::{
        Conflict, ConflictDetector, ConflictResolver, ConflictType, DependencyGraph, NodeStatus,
        ParallelConfig, ParallelResult,
    };
}
