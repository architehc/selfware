//! Concurrent queue utilities with async/await support
//!
//! This module currently exposes a repaired bounded MPSC queue wrapper.
//!
//! # Example
//!
//! ```rust,no_run
//! use selfware::concurrent_queue::BoundedQueue;
//!
//! #[tokio::main]
//! async fn main() {
//!     let queue = BoundedQueue::new(4);
//!     queue.send("work item").await.unwrap();
//!     assert_eq!(queue.receive().await, Some("work item"));
//! }
//! ```

pub use bounded::BoundedQueue;

mod bounded;
