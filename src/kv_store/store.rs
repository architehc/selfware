use crate::kv_store::entry::Entry;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

/// Error types for key-value store operations
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum KvStoreError {
    KeyNotFound(String),
    KeyAlreadyExists(String),
    StorageError(String),
    SerializationError(String),
}

impl std::fmt::Display for KvStoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            KvStoreError::KeyNotFound(key) => write!(f, "Key not found: {}", key),
            KvStoreError::KeyAlreadyExists(key) => write!(f, "Key already exists: {}", key),
            KvStoreError::StorageError(msg) => write!(f, "Storage error: {}", msg),
            KvStoreError::SerializationError(msg) => write!(f, "Serialization error: {}", msg),
        }
    }
}

impl std::error::Error for KvStoreError {}

/// Result type for key-value store operations
pub type Result<T> = std::result::Result<T, KvStoreError>;

/// A simple key-value store with persistence support
#[derive(Debug, Clone)]
pub struct KvStore {
    pub(crate) data: HashMap<String, Entry>,
    path: Option<String>,
}

impl KvStore {
    /// Create a new in-memory key-value store
    pub fn new() -> Self {
        Self {
            data: HashMap::new(),
            path: None,
        }
    }

    /// Create a new key-value store with persistence to the given path
    pub fn with_path<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path_str = path.as_ref().to_string_lossy().to_string();
        
        // Try to load existing data
        if path.as_ref().exists() {
            match Self::load(path.as_ref()) {
                Ok(store) => Ok(store),
                Err(_) => {
                    // If loading fails, create a new store
                    Ok(Self {
                        data: HashMap::new(),
                        path: Some(path_str),
                    })
                }
            }
        } else {
            // Create new store and save it
            let store = Self {
                data: HashMap::new(),
                path: Some(path_str),
            };
            store.save()?;
            Ok(store)
        }
    }

    /// Insert a key-value entry
    pub fn insert(&mut self, key: impl Into<String>, value: impl Into<String>) -> Result<()> {
        let key = key.into();
        
        if self.data.contains_key(&key) {
            return Err(KvStoreError::KeyAlreadyExists(key));
        }

        let entry = Entry::new(key.clone(), value);
        self.data.insert(key.clone(), entry);
        
        if let Some(ref path) = self.path {
            self.save()?;
        }

        Ok(())
    }

    /// Insert or update a key-value entry
    pub fn upsert(&mut self, key: impl Into<String>, value: impl Into<String>) -> Result<()> {
        let key = key.into();
        let entry = Entry::new(key.clone(), value);
        self.data.insert(key.clone(), entry);
        
        if let Some(ref path) = self.path {
            self.save()?;
        }

        Ok(())
    }

    /// Insert or update an Entry directly
    pub fn upsert_entry(&mut self, key: impl Into<String>, entry: Entry) -> Result<()> {
        let key = key.into();
        self.data.insert(key.clone(), entry);
        
        if let Some(ref path) = self.path {
            self.save()?;
        }

        Ok(())
    }

    /// Get a value by key
    pub fn get(&self, key: &str) -> Result<String> {
        match self.data.get(key) {
            Some(entry) => Ok(entry.value.clone()),
            None => Err(KvStoreError::KeyNotFound(key.to_string())),
        }
    }

    /// Get an entry by key
    pub fn get_entry(&self, key: &str) -> Result<&Entry> {
        match self.data.get(key) {
            Some(entry) => Ok(entry),
            None => Err(KvStoreError::KeyNotFound(key.to_string())),
        }
    }

    /// Check if a key exists
    pub fn contains_key(&self, key: &str) -> bool {
        self.data.contains_key(key)
    }

    /// Remove a key-value entry
    pub fn remove(&mut self, key: &str) -> Result<String> {
        match self.data.remove(key) {
            Some(entry) => {
                let value = entry.value.clone();
                if let Some(ref path) = self.path {
                    self.save()?;
                }
                Ok(value)
            }
            None => Err(KvStoreError::KeyNotFound(key.to_string())),
        }
    }

    /// Get the number of entries in the store
    pub fn len(&self) -> usize {
        self.data.len()
    }

    /// Check if the store is empty
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    /// Clear all entries from the store
    pub fn clear(&mut self) -> Result<()> {
        self.data.clear();
        if let Some(ref path) = self.path {
            self.save()?;
        }
        Ok(())
    }

    /// Save the store to disk
    pub fn save(&self) -> Result<()> {
        if let Some(ref path) = self.path {
            // Create a serializable representation
            let serializable_data: HashMap<String, String> = self.data
                .iter()
                .map(|(k, v)| (k.clone(), v.value.clone()))
                .collect();
            
            match serde_json::to_string_pretty(&serializable_data) {
                Ok(json) => {
                    if let Err(e) = fs::write(path, &json) {
                        return Err(KvStoreError::StorageError(format!(
                            "Failed to write to {}: {}",
                            path, e
                        )));
                    }
                    Ok(())
                }
                Err(e) => Err(KvStoreError::SerializationError(format!(
                    "Failed to serialize store: {}",
                    e
                ))),
            }
        } else {
            Err(KvStoreError::StorageError(
                "No path configured for persistence".to_string(),
            ))
        }
    }

    /// Load the store from disk
    pub fn load<P: AsRef<Path>>(path: P) -> Result<Self> {
        match fs::read_to_string(path.as_ref()) {
            Ok(json) => {
                match serde_json::from_str::<HashMap<String, String>>(&json) {
                    Ok(data) => {
                        let store_data: HashMap<String, Entry> = data
                            .into_iter()
                            .map(|(k, v)| (k.clone(), Entry::new(k.clone(), v)))
                            .collect();
                        
                        Ok(Self {
                            data: store_data,
                            path: Some(path.as_ref().to_string_lossy().to_string()),
                        })
                    }
                    Err(e) => Err(KvStoreError::SerializationError(format!(
                        "Failed to deserialize store: {}",
                        e
                    ))),
                }
            }
            Err(e) => Err(KvStoreError::StorageError(format!(
                "Failed to read from {}: {}",
                path.as_ref().display(),
                e
            ))),
        }
    }

    /// Get all keys in the store
    pub fn keys(&self) -> Vec<String> {
        self.data.keys().cloned().collect()
    }

    /// Get all entries in the store
    pub fn entries(&self) -> Vec<&Entry> {
        self.data.values().collect()
    }

    /// Iterate over all key-value pairs
    pub fn iter(&self) -> impl Iterator<Item = (&String, &Entry)> {
        self.data.iter()
    }
}

impl Default for KvStore {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_new_store_is_empty() {
        let store = KvStore::new();
        assert!(store.is_empty());
        assert_eq!(store.len(), 0);
    }

    #[test]
    fn test_insert_and_get() {
        let mut store = KvStore::new();
        store.insert("key1", "value1").unwrap();
        
        assert_eq!(store.get("key1").unwrap(), "value1");
        assert_eq!(store.len(), 1);
    }

    #[test]
    fn test_insert_duplicate_key() {
        let mut store = KvStore::new();
        store.insert("key1", "value1").unwrap();
        
        // Inserting the same key should fail
        assert!(store.insert("key1", "value2").is_err());
        assert_eq!(store.get("key1").unwrap(), "value1");
    }

    #[test]
    fn test_upsert() {
        let mut store = KvStore::new();
        store.insert("key1", "value1").unwrap();
        
        // Upsert should update the value
        store.upsert("key1", "value2").unwrap();
        assert_eq!(store.get("key1").unwrap(), "value2");
    }

    #[test]
    fn test_remove() {
        let mut store = KvStore::new();
        store.insert("key1", "value1").unwrap();
        
        assert_eq!(store.remove("key1").unwrap(), "value1");
        assert!(store.get("key1").is_err());
        assert_eq!(store.len(), 0);
    }

    #[test]
    fn test_contains_key() {
        let mut store = KvStore::new();
        store.insert("key1", "value1").unwrap();
        
        assert!(store.contains_key("key1"));
        assert!(!store.contains_key("key2"));
    }

    #[test]
    fn test_get_nonexistent_key() {
        let store = KvStore::new();
        
        assert!(store.get("nonexistent").is_err());
    }

    #[test]
    fn test_remove_nonexistent_key() {
        let mut store = KvStore::new();
        
        assert!(store.remove("nonexistent").is_err());
    }

    #[test]
    fn test_clear() {
        let mut store = KvStore::new();
        store.insert("key1", "value1").unwrap();
        store.insert("key2", "value2").unwrap();
        
        store.clear().unwrap();
        assert!(store.is_empty());
        assert_eq!(store.len(), 0);
    }

    #[test]
    fn test_persistence() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("store.json");
        
        let mut store = KvStore::with_path(&path).unwrap();
        store.insert("key1", "value1").unwrap();
        store.insert("key2", "value2").unwrap();
        
        // Create a new store from the same path
        let store2 = KvStore::with_path(&path).unwrap();
        assert_eq!(store2.get("key1").unwrap(), "value1");
        assert_eq!(store2.get("key2").unwrap(), "value2");
    }

    #[test]
    fn test_keys() {
        let mut store = KvStore::new();
        store.insert("key1", "value1").unwrap();
        store.insert("key2", "value2").unwrap();
        
        let keys = store.keys();
        assert!(keys.contains(&"key1".to_string()));
        assert!(keys.contains(&"key2".to_string()));
    }

    #[test]
    fn test_entries() {
        let mut store = KvStore::new();
        store.insert("key1", "value1").unwrap();
        store.insert("key2", "value2").unwrap();
        
        let entries = store.entries();
        assert_eq!(entries.len(), 2);
    }
}