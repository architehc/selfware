use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Represents a single key-value entry in the store
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entry {
    pub key: String,
    pub value: String,
    pub tags: Vec<String>,
    pub metadata: HashMap<String, String>,
}

impl Entry {
    pub fn new(key: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            value: value.into(),
            tags: Vec::new(),
            metadata: HashMap::new(),
        }
    }

    pub fn with_tags(mut self, tags: Vec<String>) -> Self {
        self.tags = tags;
        self
    }

    pub fn with_metadata(mut self, metadata: HashMap<String, String>) -> Self {
        self.metadata = metadata;
        self
    }

    pub fn add_tag(&mut self, tag: impl Into<String>) {
        self.tags.push(tag.into());
    }

    pub fn add_metadata(&mut self, key: impl Into<String>, value: impl Into<String>) {
        self.metadata.insert(key.into(), value.into());
    }

    pub fn has_tag(&self, tag: &str) -> bool {
        self.tags.iter().any(|t| t == tag)
    }

    pub fn get_metadata(&self, key: &str) -> Option<&String> {
        self.metadata.get(key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn test_entry_creation() {
        let entry = Entry::new("key1", "value1");
        assert_eq!(entry.key, "key1");
        assert_eq!(entry.value, "value1");
        assert!(entry.tags.is_empty());
        assert!(entry.metadata.is_empty());
    }

    #[test]
    fn test_entry_with_tags() {
        let entry =
            Entry::new("key1", "value1").with_tags(vec!["tag1".to_string(), "tag2".to_string()]);
        assert_eq!(entry.tags.len(), 2);
        assert!(entry.has_tag("tag1"));
        assert!(entry.has_tag("tag2"));
    }

    #[test]
    fn test_entry_with_metadata() {
        let mut metadata = HashMap::new();
        metadata.insert("created_by".to_string(), "user".to_string());
        let entry = Entry::new("key1", "value1").with_metadata(metadata);
        assert_eq!(entry.get_metadata("created_by"), Some(&"user".to_string()));
    }

    #[test]
    fn test_entry_mutators() {
        let mut entry = Entry::new("key1", "value1");
        entry.add_tag("new_tag");
        assert!(entry.has_tag("new_tag"));

        entry.add_metadata("key", "value");
        assert_eq!(entry.get_metadata("key"), Some(&"value".to_string()));
    }
}
