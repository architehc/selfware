use crate::kv_store::entry::Entry;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Serialization format for storing Entry data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerializedEntry {
    pub key: String,
    pub value: String,
    pub tags: Vec<String>,
    pub metadata: HashMap<String, String>,
}

impl From<&Entry> for SerializedEntry {
    fn from(entry: &Entry) -> Self {
        Self {
            key: entry.key.clone(),
            value: entry.value.clone(),
            tags: entry.tags.clone(),
            metadata: entry.metadata.clone(),
        }
    }
}

impl From<&SerializedEntry> for Entry {
    fn from(serialized: &SerializedEntry) -> Self {
        Self {
            key: serialized.key.clone(),
            value: serialized.value.clone(),
            tags: serialized.tags.clone(),
            metadata: serialized.metadata.clone(),
        }
    }
}

/// Serialization utilities for KvStore
pub trait StoreSerializer {
    /// Serialize the entire store to a JSON string
    fn serialize_to_json(&self) -> Result<String, String>;

    /// Deserialize the entire store from a JSON string
    fn deserialize_from_json(json: &str) -> Result<HashMap<String, Entry>, String>;

    /// Serialize a single entry to JSON
    fn serialize_entry_to_json(entry: &Entry) -> Result<String, String>;

    /// Deserialize a single entry from JSON
    fn deserialize_entry_from_json(json: &str) -> Result<Entry, String>;
}

/// Default implementation for serialization
impl StoreSerializer for HashMap<String, Entry> {
    fn serialize_to_json(&self) -> Result<String, String> {
        let serialized: Vec<SerializedEntry> = self.values().map(SerializedEntry::from).collect();
        serde_json::to_string_pretty(&serialized).map_err(|e| format!("Serialization error: {}", e))
    }

    fn deserialize_from_json(json: &str) -> Result<HashMap<String, Entry>, String> {
        let entries: Vec<SerializedEntry> =
            serde_json::from_str(json).map_err(|e| format!("Deserialization error: {}", e))?;

        let mut store = HashMap::new();
        for entry in entries {
            let e: Entry = Entry::from(&entry);
            store.insert(e.key.clone(), e);
        }

        Ok(store)
    }

    fn serialize_entry_to_json(entry: &Entry) -> Result<String, String> {
        let serialized = SerializedEntry::from(entry);
        serde_json::to_string(&serialized).map_err(|e| format!("Serialization error: {}", e))
    }

    fn deserialize_entry_from_json(json: &str) -> Result<Entry, String> {
        let serialized: SerializedEntry =
            serde_json::from_str(json).map_err(|e| format!("Deserialization error: {}", e))?;

        Ok(Entry::from(&serialized))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kv_store::entry::Entry;
    use std::collections::HashMap;

    #[test]
    fn test_serialize_entry() {
        let mut metadata = HashMap::new();
        metadata.insert("author".to_string(), "test".to_string());

        let entry = Entry::new("key1", "value1")
            .with_tags(vec!["tag1".to_string()])
            .with_metadata(metadata);

        let json =
            <HashMap<String, Entry> as StoreSerializer>::serialize_entry_to_json(&entry).unwrap();
        assert!(json.contains("\"key\":\"key1\""));
        assert!(json.contains("\"value\":\"value1\""));

        // Verify deserialization roundtrip
        let deserialized =
            <HashMap<String, Entry> as StoreSerializer>::deserialize_entry_from_json(&json)
                .unwrap();
        assert_eq!(deserialized.key, entry.key);
        assert_eq!(deserialized.value, entry.value);
    }

    #[test]
    fn test_deserialize_entry() {
        let json =
            r#"{"key":"key1","value":"value1","tags":["tag1"],"metadata":{"author":"test"}}"#;

        let entry =
            <HashMap<String, Entry> as StoreSerializer>::deserialize_entry_from_json(json).unwrap();
        assert_eq!(entry.key, "key1");
        assert_eq!(entry.value, "value1");
        assert!(entry.has_tag("tag1"));
        assert_eq!(entry.get_metadata("author"), Some(&"test".to_string()));
    }

    #[test]
    fn test_serialize_store() {
        let mut store = HashMap::new();

        let entry1 = Entry::new("key1", "value1");
        let entry2 = Entry::new("key2", "value2");

        store.insert("key1".to_string(), entry1);
        store.insert("key2".to_string(), entry2);

        let json = <HashMap<String, Entry> as StoreSerializer>::serialize_to_json(&store).unwrap();
        assert!(json.contains("key1"));
        assert!(json.contains("key2"));
        assert!(json.contains("value1"));
        assert!(json.contains("value2"));
    }

    #[test]
    fn test_deserialize_store() {
        let json = r#"[{"key":"key1","value":"value1","tags":[],"metadata":{}},{"key":"key2","value":"value2","tags":[],"metadata":{}}]"#;

        let store =
            <HashMap<String, Entry> as StoreSerializer>::deserialize_from_json(json).unwrap();
        assert_eq!(store.len(), 2);
        assert!(store.contains_key("key1"));
        assert!(store.contains_key("key2"));
    }
}
