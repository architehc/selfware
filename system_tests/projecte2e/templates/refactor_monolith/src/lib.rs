use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ---------------------------------------------------------------------------
// Entry
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Entry {
    pub key: String,
    pub value: String,
    pub created_at: u64,
    pub updated_at: u64,
    pub tags: Vec<String>,
}

impl Entry {
    pub fn new(key: &str, value: &str, timestamp: u64) -> Self {
        Self {
            key: key.to_string(),
            value: value.to_string(),
            created_at: timestamp,
            updated_at: timestamp,
            tags: Vec::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// KvStore
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KvStore {
    entries: HashMap<String, Entry>,
    #[serde(skip)]
    next_ts: u64,
}

impl KvStore {
    /// Create an empty store. The internal timestamp counter starts at 1.
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
            next_ts: 1,
        }
    }

    fn tick(&mut self) -> u64 {
        let ts = self.next_ts;
        self.next_ts += 1;
        ts
    }

    // -- CRUD ---------------------------------------------------------------

    /// Insert or update an entry. Tags are preserved on update.
    pub fn set(&mut self, key: &str, value: &str) {
        let ts = self.tick();
        self.entries
            .entry(key.to_string())
            .and_modify(|e| {
                e.value = value.to_string();
                e.updated_at = ts;
            })
            .or_insert_with(|| Entry::new(key, value, ts));
    }

    /// Return the value for `key`, if it exists.
    pub fn get(&self, key: &str) -> Option<&str> {
        self.entries.get(key).map(|e| e.value.as_str())
    }

    /// Remove an entry. Returns `true` if the key existed.
    pub fn delete(&mut self, key: &str) -> bool {
        self.entries.remove(key).is_some()
    }

    /// Return all keys in arbitrary order.
    pub fn keys(&self) -> Vec<&str> {
        self.entries.keys().map(|k| k.as_str()).collect()
    }

    /// Number of entries.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the store is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    // -- Queries ------------------------------------------------------------

    /// Return all entries that carry `tag`.
    pub fn find_by_tag(&self, tag: &str) -> Vec<&Entry> {
        self.entries
            .values()
            .filter(|e| e.tags.iter().any(|t| t == tag))
            .collect()
    }

    /// Return all entries whose key starts with `prefix`.
    pub fn find_by_prefix(&self, prefix: &str) -> Vec<&Entry> {
        self.entries
            .values()
            .filter(|e| e.key.starts_with(prefix))
            .collect()
    }

    /// Return all entries whose `updated_at` is strictly greater than `ts`.
    pub fn find_newer_than(&self, ts: u64) -> Vec<&Entry> {
        self.entries
            .values()
            .filter(|e| e.updated_at > ts)
            .collect()
    }

    // -- Tags ---------------------------------------------------------------

    /// Add `tag` to the entry at `key`. Returns `false` if the key does not
    /// exist or the tag is already present.
    pub fn add_tag(&mut self, key: &str, tag: &str) -> bool {
        if let Some(entry) = self.entries.get_mut(key) {
            if entry.tags.iter().any(|t| t == tag) {
                return false;
            }
            entry.tags.push(tag.to_string());
            true
        } else {
            false
        }
    }

    /// Remove `tag` from the entry at `key`. Returns `false` if the key does
    /// not exist or the tag was not present.
    pub fn remove_tag(&mut self, key: &str, tag: &str) -> bool {
        if let Some(entry) = self.entries.get_mut(key) {
            let before = entry.tags.len();
            entry.tags.retain(|t| t != tag);
            entry.tags.len() < before
        } else {
            false
        }
    }

    // -- Serialization ------------------------------------------------------

    /// Serialize the store to a JSON string.
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).expect("serialization should not fail")
    }

    /// Deserialize a store from a JSON string.
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }

    // -- Merge --------------------------------------------------------------

    /// Merge `other` into `self`. For duplicate keys the entry with the later
    /// `updated_at` wins. If timestamps are equal the entry from `other` wins.
    pub fn merge(&mut self, other: &KvStore) {
        for (key, other_entry) in &other.entries {
            match self.entries.get(key) {
                Some(existing) if existing.updated_at > other_entry.updated_at => {
                    // keep ours
                }
                _ => {
                    self.entries.insert(key.clone(), other_entry.clone());
                }
            }
        }
    }
}

impl Default for KvStore {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_set_and_get() {
        let mut store = KvStore::new();
        store.set("name", "Alice");
        assert_eq!(store.get("name"), Some("Alice"));

        // overwrite
        store.set("name", "Bob");
        assert_eq!(store.get("name"), Some("Bob"));

        // missing key
        assert_eq!(store.get("missing"), None);
    }

    #[test]
    fn test_delete() {
        let mut store = KvStore::new();
        store.set("a", "1");
        assert!(store.delete("a"));
        assert!(!store.delete("a"));
        assert!(store.is_empty());
    }

    #[test]
    fn test_keys() {
        let mut store = KvStore::new();
        store.set("x", "1");
        store.set("y", "2");
        let mut keys = store.keys();
        keys.sort();
        assert_eq!(keys, vec!["x", "y"]);
        assert_eq!(store.len(), 2);
    }

    #[test]
    fn test_tags() {
        let mut store = KvStore::new();
        store.set("color", "red");

        assert!(store.add_tag("color", "primary"));
        assert!(!store.add_tag("color", "primary")); // duplicate
        assert!(!store.add_tag("missing", "x"));

        let found = store.find_by_tag("primary");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].key, "color");

        assert!(store.remove_tag("color", "primary"));
        assert!(!store.remove_tag("color", "primary")); // already gone
        assert!(store.find_by_tag("primary").is_empty());
    }

    #[test]
    fn test_find_by_prefix() {
        let mut store = KvStore::new();
        store.set("user:1", "Alice");
        store.set("user:2", "Bob");
        store.set("item:1", "Sword");

        let users = store.find_by_prefix("user:");
        assert_eq!(users.len(), 2);
    }

    #[test]
    fn test_find_newer_than() {
        let mut store = KvStore::new();
        store.set("a", "1"); // ts 1
        store.set("b", "2"); // ts 2
        store.set("c", "3"); // ts 3

        let newer = store.find_newer_than(1);
        assert_eq!(newer.len(), 2);
    }

    #[test]
    fn test_json_roundtrip() {
        let mut store = KvStore::new();
        store.set("k", "v");
        store.add_tag("k", "important");

        let json = store.to_json();
        let restored = KvStore::from_json(&json).unwrap();

        assert_eq!(restored.get("k"), Some("v"));
        assert_eq!(restored.find_by_tag("important").len(), 1);
    }

    #[test]
    fn test_merge() {
        let mut a = KvStore::new();
        a.set("x", "old"); // ts 1

        let mut b = KvStore::new();
        b.set("x", "new"); // ts 1 in b's counter — same ts, other wins
        b.set("y", "only_b"); // ts 2

        a.merge(&b);

        assert_eq!(a.get("x"), Some("new"));
        assert_eq!(a.get("y"), Some("only_b"));
    }
}
