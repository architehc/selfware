//! Bounded concurrent queue implementation
//!
//! A thread-safe bounded queue that supports multiple producers and consumers
//! with backpressure when the queue is full.

use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::sync::Mutex;

/// A bounded queue that supports multiple producers and consumers
#[derive(Clone)]
pub struct BoundedQueue<T> {
    sender: mpsc::Sender<T>,
    receiver: Arc<Mutex<mpsc::Receiver<T>>>,
    capacity: usize,
}

impl<T: Send + 'static> BoundedQueue<T> {
    /// Create a new bounded queue with the specified capacity
    pub fn new(capacity: usize) -> Self {
        let (sender, receiver) = mpsc::channel(capacity);
        Self {
            sender,
            receiver: Arc::new(Mutex::new(receiver)),
            capacity,
        }
    }

    /// Send an item to the queue
    /// Returns an error if the queue is closed
    pub async fn send(&self, item: T) -> Result<(), mpsc::error::SendError<T>> {
        self.sender.send(item).await
    }

    /// Try to send an item without blocking
    pub fn try_send(&self, item: T) -> Result<(), mpsc::error::TrySendError<T>> {
        self.sender.try_send(item)
    }

    /// Receive an item from the queue
    /// Returns None if the queue is closed and empty
    pub async fn receive(&self) -> Option<T> {
        let mut receiver = self.receiver.lock().await;
        receiver.recv().await
    }

    /// Try to receive an item without blocking
    pub fn try_receive(&self) -> Option<T> {
        let receiver = self.receiver.blocking_lock();
        receiver.try_recv().ok()
    }

    /// Get the current capacity of the queue
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// Get the approximate number of items in the queue
    pub fn len(&self) -> usize {
        self.sender.capacity().unwrap_or(0)
    }

    /// Check if the queue is empty
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Close the queue, preventing new items from being sent
    pub async fn close(&self) {
        self.sender.close();
    }

    /// Get a clone of the sender for creating additional producers
    pub fn sender(&self) -> mpsc::Sender<T> {
        self.sender.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::{timeout, Duration};

    #[tokio::test]
    async fn test_bounded_queue_basic() {
        let queue: BoundedQueue<i32> = BoundedQueue::new(10);
        
        queue.send(1).await.unwrap();
        queue.send(2).await.unwrap();
        
        assert_eq!(queue.receive().await, Some(1));
        assert_eq!(queue.receive().await, Some(2));
        assert_eq!(queue.receive().await, None);
    }

    #[tokio::test]
    async fn test_bounded_queue_capacity() {
        let queue: BoundedQueue<i32> = BoundedQueue::new(2);
        
        queue.send(1).await.unwrap();
        queue.send(2).await.unwrap();
        
        // Third send should fail as queue is full
        let result = timeout(Duration::from_millis(100), queue.send(3)).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_bounded_queue_try_send() {
        let queue: BoundedQueue<i32> = BoundedQueue::new(1);
        
        assert!(queue.try_send(1).is_ok());
        assert!(queue.try_send(2).is_err());
        
        queue.try_receive();
        assert!(queue.try_send(2).is_ok());
    }

    #[tokio::test]
    async fn test_bounded_queue_close() {
        let queue: BoundedQueue<i32> = BoundedQueue::new(10);
        let queue2 = queue.clone();
        
        queue.close().await;
        
        let result = timeout(Duration::from_millis(100), queue2.send(1)).await;
        assert!(result.is_err());
    }
}
