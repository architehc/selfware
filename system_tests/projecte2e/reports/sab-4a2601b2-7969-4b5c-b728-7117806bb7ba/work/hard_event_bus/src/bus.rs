use crate::event::Event;
use crate::subscriber::Subscriber;
use std::sync::atomic::{AtomicU64, Ordering};

/// A publish-subscribe event bus.
pub struct EventBus {
    subscribers: Vec<Subscriber>,
    /// Next sequence number for events.
    next_seq: AtomicU64,
    /// Total events published.
    published_count: AtomicU64,
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new()
    }
}

impl EventBus {
    pub fn new() -> Self {
        Self {
            subscribers: Vec::new(),
            next_seq: AtomicU64::new(1),
            published_count: AtomicU64::new(0),
        }
    }

    /// Register a subscriber.
    pub fn subscribe(&mut self, subscriber: Subscriber) {
        self.subscribers.push(subscriber);
    }

    /// Publish an event to all matching subscribers.
    pub fn publish(&self, mut event: Event) {
        let seq = self.next_seq.fetch_add(1, Ordering::Relaxed);
        event.seq = seq;
        self.published_count.fetch_add(1, Ordering::Relaxed);

        for sub in &self.subscribers {
            if sub.matches(&event.topic) {
                sub.deliver(event.clone());
            }
        }
    }

    /// How many events have been published.
    pub fn published_count(&self) -> u64 {
        self.published_count.load(Ordering::Relaxed)
    }

    /// Current sequence number (next to be assigned).
    pub fn current_seq(&self) -> u64 {
        self.next_seq.load(Ordering::Relaxed)
    }

    /// Number of registered subscribers.
    pub fn subscriber_count(&self) -> usize {
        self.subscribers.len()
    }
}
