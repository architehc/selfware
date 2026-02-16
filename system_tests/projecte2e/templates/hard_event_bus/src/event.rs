use std::collections::HashMap;
use std::fmt;

/// An event that can be published on the bus.
#[derive(Debug, Clone)]
pub struct Event {
    /// Topic of this event (e.g. "user.created").
    pub topic: String,
    /// Key-value payload.
    pub data: HashMap<String, String>,
    /// Monotonic sequence number assigned by the bus.
    pub seq: u64,
}

impl Event {
    pub fn new(topic: impl Into<String>) -> Self {
        Self {
            topic: topic.into(),
            data: HashMap::new(),
            seq: 0,
        }
    }

    pub fn with_data(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.data.insert(key.into(), value.into());
        self
    }
}

/// BUG: Display implementation shows wrong format — uses Debug instead of
/// "Event(topic, seq=N)" format the tests expect.
impl fmt::Display for Event {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}
