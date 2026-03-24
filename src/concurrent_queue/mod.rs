//! Concurrent Task Queue with async/await support
//!
//! This module provides a highly concurrent, thread-safe task queue implementation
//! designed for async Rust applications. It supports:
//! - Priority-based task scheduling
//! - Multiple workers processing tasks concurrently
//! - Graceful shutdown and cancellation
//! - Task dependencies and ordering
//! - Metrics and observability
//!
//! # Example
//!
//! ```rust,no_run
//! use selfware::concurrent_queue::{TaskQueue, TaskPriority, TaskResult};
//! use std::time::Duration;
//!
//! #[tokio::main]
//! async fn main() {
//!     let queue = TaskQueue::new(4); // 4 workers
//!     
//!     // Submit high priority task
//!     queue.submit(TaskPriority::High, async {
//!         "High priority result"
//!     }).await;
//!     
//!     // Submit normal priority task
//!     queue.submit(TaskPriority::Normal, async {
//!         tokio::time::sleep(Duration::from_millis(100)).await;
//!         "Normal task completed"
//!     }).await;
//!     
//!     // Wait for all tasks to complete
//!     queue.shutdown().await;
//! }
//! ```

pub use bounded::BoundedQueue;
pub use priority::PriorityQueue;
pub use task_queue::{TaskQueue, TaskConfig, TaskResult, TaskError, TaskPriority, TaskHandle};
pub use worker_pool::{WorkerPool, WorkerConfig, WorkerStats};
pub use metrics::{QueueMetrics, TaskMetrics};

mod bounded;
mod priority;
mod task_queue;
mod worker_pool;
mod metrics;
