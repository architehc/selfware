use crate::entry::Entry;
use std::collections::HashMap;

/// A key-value store with support for tags and metadata
#[derive(Debug, Clone)]
pub struct KvStore {
    entries: HashMap<String, Entry>,
}

impl KvStore {
    /// Creates a new empty store
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
        }
    }

    /// Creates a store from an existing HashMap
    pub fn from_map(entries: HashMap<String, Entry>) -> Self {
        Self { entries }
    }

    /// Inserts a new entry into the store
    pub fn insert(&mut self, entry: Entry) -> Option<Entry> {
        let key = entry.key.clone();
        self.entries.insert(key, entry)
    }

    /// Retrieves an entry by key
    pub fn get(&self, key: &str) -> Option<&Entry> {
        self.entries.get(key)
    }

    /// Removes an entry by key and returns it
    pub fn remove(&mut self, key: &str) -> Option<Entry> {
        self.entries.remove(key)
    }

    /// Checks if an entry with the given key exists
    pub fn contains_key(&self, key: &str) -> bool {
        self.entries.contains_key(key)
    }

    /// Returns the number of entries in the store
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Returns true if the store is empty
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Returns an iterator over all entries
    pub fn iter(&self) -> impl Iterator<Item = &Entry> {
        self.entries.values()
    }

    /// Returns an iterator over all keys
    pub fn keys(&self) -> impl Iterator<Item = &String> {
        self.entries.keys()
    }

    /// Clears all entries from the store
    pub fn clear(&mut self) {
        self.entries.clear();
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
    use crate::entry::Entry;

    #[test]
    fn test_store_creation() {
        let store = KvStore::new();
        assert!(store.is_empty());
        assert_eq!(store.len(), 0);
    }

    #[test]
    fn test_insert_and_get() {
        let mut store = KvStore::new();
        let entry = Entry::new("key1", "value1");
        
        store.insert(entry.clone());
        assert_eq!(store.len(), 1);
        assert!(store.contains_key("key1"));
        
        let retrieved = store.get("key1");
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().value, "value1");
    }

    #[test]
    fn test_remove() {
        let mut store = KvStore::new();
        let entry = Entry::new("key1", "value1");
        
        store.insert(entry);
        assert!(store.contains_key("key1"));
        
        let removed = store.remove("key1");
        assert!(removed.is_some());
        assert!(!store.contains_key("key1"));
        assert!(store.is_empty());
    }

    #[test]
    fn test_iter() {
        let mut store = KvStore::new();
        store.insert(Entry::new("key1", "value1"));
        store.insert(Entry::new("key2", "value2"));
        store.insert(Entry::new("key3", "value3"));
        
        let count = store.iter().count();
        assert_eq!(count, 3);
    }

    #[test]
    fn test_clear() {
        let mut store = KvStore::new();
        store.insert(Entry::new("key1", "value1"));
        store.insert(Entry::new("key2", "value2"));
        
        assert!(!store.is_empty());
        
        store.clear();
        assert!(store.is_empty());
    }

    #[test]
    fn test_from_map() {
        let mut map = HashMap::new();
        map.insert("key1".to_string(), Entry::new("key1", "value1"));
        
        let store = KvStore::from_map(map);
        assert_eq!(store.len(), 1);
        assert!(store.contains_key("key1"));
    }
}