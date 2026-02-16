use crate::event::Event;
use std::sync::{Arc, Mutex};

/// A subscriber that receives events matching a topic filter.
pub struct Subscriber {
    /// Name of this subscriber.
    pub name: String,
    /// Topic filter — only events whose topic starts with this prefix are delivered.
    filter: String,
    /// Received events.
    received: Arc<Mutex<Vec<Event>>>,
}

impl Subscriber {
    pub fn new(name: impl Into<String>, filter: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            filter: filter.into(),
            received: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Check if this subscriber wants events on `topic`.
    ///
    /// BUG: uses exact equality instead of prefix matching.
    pub fn matches(&self, topic: &str) -> bool {
        self.filter == topic // BUG: should be topic.starts_with(&self.filter)
    }

    /// Deliver an event to this subscriber.
    pub fn deliver(&self, event: Event) {
        if let Ok(mut received) = self.received.lock() {
            received.push(event);
        }
    }

    /// Get a snapshot of all received events.
    pub fn received(&self) -> Vec<Event> {
        self.received
            .lock()
            .map(|r| r.clone())
            .unwrap_or_default()
    }

    /// Number of events received.
    pub fn count(&self) -> usize {
        self.received
            .lock()
            .map(|r| r.len())
            .unwrap_or(0)
    }
}
