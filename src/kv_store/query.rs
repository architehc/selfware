use super::entry::Entry;
use super::store::KvStore;
use std::collections::HashMap;

/// Query methods for the key-value store
impl KvStore {
    /// Get all entries with a specific tag
    pub fn by_tag(&self, tag: &str) -> Vec<&Entry> {
        self.data
            .values()
            .filter(|entry| entry.has_tag(tag))
            .collect()
    }

    /// Get all entries matching a metadata key-value pair
    pub fn by_metadata(&self, key: &str, value: &str) -> Vec<&Entry> {
        self.data
            .values()
            .filter(|entry| entry.get_metadata(key).map(|v| v.as_str()) == Some(value))
            .collect()
    }

    /// Get all entries matching multiple metadata conditions
    pub fn by_metadata_multi(
        &self,
        conditions: &[(&str, &str)],
    ) -> Vec<&Entry> {
        self.data
            .values()
            .filter(|entry| {
                conditions
                    .iter()
                    .all(|(k, v)| entry.get_metadata(k).map(|val| val.as_str()) == Some(*v))
            })
            .collect()
    }

    /// Get entries by tag with metadata filter
    pub fn by_tag_and_metadata(
        &self,
        tag: &str,
        metadata_key: &str,
        metadata_value: &str,
    ) -> Vec<&Entry> {
        self.data
            .values()
            .filter(|entry| {
                entry.has_tag(tag) && entry.get_metadata(metadata_key).map(|v| v.as_str()) == Some(metadata_value)
            })
            .collect()
    }

    /// Find entries by key prefix
    pub fn by_key_prefix(&self, prefix: &str) -> Vec<(&String, &Entry)> {
        self.data
            .iter()
            .filter(|(key, _)| key.starts_with(prefix))
            .collect()
    }

    /// Find entries by key suffix
    pub fn by_key_suffix(&self, suffix: &str) -> Vec<(&String, &Entry)> {
        self.data
            .iter()
            .filter(|(key, _)| key.ends_with(suffix))
            .collect()
    }

    /// Find entries by key pattern (contains)
    pub fn by_key_contains(&self, pattern: &str) -> Vec<(&String, &Entry)> {
        self.data
            .iter()
            .filter(|(key, _)| key.contains(pattern))
            .collect()
    }

    /// Get all unique tags across all entries
    pub fn all_tags(&self) -> Vec<String> {
        let mut tags = HashMap::new();
        for entry in self.data.values() {
            for tag in &entry.tags {
                *tags.entry(tag.clone()).or_insert(0) += 1;
            }
        }
        let mut result: Vec<String> = tags.keys().cloned().collect();
        result.sort();
        result
    }

    /// Get all unique metadata keys across all entries
    pub fn all_metadata_keys(&self) -> Vec<String> {
        let mut keys = HashMap::new();
        for entry in self.data.values() {
            for key in entry.metadata.keys() {
                *keys.entry(key.clone()).or_insert(0) += 1;
            }
        }
        let mut result: Vec<String> = keys.keys().cloned().collect();
        result.sort();
        result
    }

    /// Get entries by a custom filter function
    pub fn filter<F>(&self, predicate: F) -> Vec<&Entry>
    where
        F: Fn(&Entry) -> bool,
    {
        self.data.values().filter(|e| predicate(e)).collect()
    }

    /// Count entries matching a tag
    pub fn count_by_tag(&self, tag: &str) -> usize {
        self.data
            .values()
            .filter(|entry| entry.has_tag(tag))
            .count()
    }

    /// Count entries matching metadata
    pub fn count_by_metadata(&self, key: &str, value: &str) -> usize {
        self.data
            .values()
            .filter(|entry| entry.get_metadata(key).map(|v| v.as_str()) == Some(value))
            .count()
    }

    /// Get entries sorted by key
    pub fn sorted_by_key(&self) -> Vec<(&String, &Entry)> {
        let mut items: Vec<(&String, &Entry)> = self.data.iter().collect();
        items.sort_by(|a, b| a.0.cmp(b.0));
        items
    }

    /// Get entries sorted by value length
    pub fn sorted_by_value_length(&self) -> Vec<(&String, &Entry)> {
        let mut items: Vec<(&String, &Entry)> = self.data.iter().collect();
        items.sort_by(|a, b| a.1.value.len().cmp(&b.1.value.len()));
        items
    }

    /// Get entries with the longest values
    pub fn longest_values(&self, limit: usize) -> Vec<(&String, &Entry)> {
        let mut items: Vec<(&String, &Entry)> = self.data.iter().collect();
        items.sort_by(|a, b| b.1.value.len().cmp(&a.1.value.len()));
        items.into_iter().take(limit).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn test_by_tag() {
        let mut store = KvStore::new();
        
        let mut entry1_metadata = HashMap::new();
        entry1_metadata.insert("type".to_string(), "file".to_string());
        let entry1 = Entry::new("key1", "value1")
            .with_tags(vec!["important".to_string(), "config".to_string()])
            .with_metadata(entry1_metadata.clone());
        
        let mut entry2_metadata = HashMap::new();
        entry2_metadata.insert("type".to_string(), "cache".to_string());
        let entry2 = Entry::new("key2", "value2")
            .with_tags(vec!["temp".to_string()])
            .with_metadata(entry2_metadata);
        
        store.upsert_entry("key1", entry1).unwrap();
        store.upsert_entry("key2", entry2).unwrap();

        let important_entries = store.by_tag("important");
        assert_eq!(important_entries.len(), 1);
        
        let config_entries = store.by_tag("config");
        assert_eq!(config_entries.len(), 1);
    }

    #[test]
    fn test_by_metadata() {
        let mut store = KvStore::new();
        
        let mut metadata1 = HashMap::new();
        metadata1.insert("author".to_string(), "alice".to_string());
        let entry1 = Entry::new("key1", "value1").with_metadata(metadata1);
        
        let mut metadata2 = HashMap::new();
        metadata2.insert("author".to_string(), "bob".to_string());
        let entry2 = Entry::new("key2", "value2").with_metadata(metadata2);
        
        store.upsert_entry("key1", entry1).unwrap();
        store.upsert_entry("key2", entry2).unwrap();

        let alice_entries = store.by_metadata("author", "alice");
        assert_eq!(alice_entries.len(), 1);
        
        let bob_entries = store.by_metadata("author", "bob");
        assert_eq!(bob_entries.len(), 1);
    }

    #[test]
    fn test_by_metadata_multi() {
        let mut store = KvStore::new();
        
        let mut metadata1 = HashMap::new();
        metadata1.insert("author".to_string(), "alice".to_string());
        metadata1.insert("type".to_string(), "config".to_string());
        let entry1 = Entry::new("key1", "value1").with_metadata(metadata1);
        
        let mut metadata2 = HashMap::new();
        metadata2.insert("author".to_string(), "alice".to_string());
        metadata2.insert("type".to_string(), "cache".to_string());
        let entry2 = Entry::new("key2", "value2").with_metadata(metadata2);
        
        store.upsert_entry("key1", entry1).unwrap();
        store.upsert_entry("key2", entry2).unwrap();

        let conditions = [("author", "alice"), ("type", "config")];
        let results = store.by_metadata_multi(&conditions);
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn test_by_key_prefix() {
        let mut store = KvStore::new();
        store.upsert("user:1", "alice").unwrap();
        store.upsert("user:2", "bob").unwrap();
        store.upsert("session:1", "active").unwrap();

        let user_entries = store.by_key_prefix("user:");
        assert_eq!(user_entries.len(), 2);
    }

    #[test]
    fn test_by_key_suffix() {
        let mut store = KvStore::new();
        store.upsert("config.json", "json data").unwrap();
        store.upsert("config.yaml", "yaml data").unwrap();
        store.upsert("data.json", "other data").unwrap();

        let json_entries = store.by_key_suffix(".json");
        assert_eq!(json_entries.len(), 2);
    }

    #[test]
    fn test_by_key_contains() {
        let mut store = KvStore::new();
        store.upsert("user_profile", "data1").unwrap();
        store.upsert("admin_profile", "data2").unwrap();
        store.upsert("session_token", "data3").unwrap();

        let profile_entries = store.by_key_contains("profile");
        assert_eq!(profile_entries.len(), 2);
    }

    #[test]
    fn test_all_tags() {
        let mut store = KvStore::new();
        let entry1 = Entry::new("key1", "value1").with_tags(vec!["a".to_string(), "b".to_string()]);
        let entry2 = Entry::new("key2", "value2").with_tags(vec!["b".to_string(), "c".to_string()]);
        store.upsert_entry("key1", entry1).unwrap();
        store.upsert_entry("key2", entry2).unwrap();

        let tags = store.all_tags();
        assert_eq!(tags, vec!["a", "b", "c"]);
    }

    #[test]
    fn test_sorted_by_key() {
        let mut store = KvStore::new();
        store.upsert("c", "value3").unwrap();
        store.upsert("a", "value1").unwrap();
        store.upsert("b", "value2").unwrap();

        let sorted = store.sorted_by_key();
        assert_eq!(sorted[0].0, "a");
        assert_eq!(sorted[1].0, "b");
        assert_eq!(sorted[2].0, "c");
    }

    #[test]
    fn test_longest_values() {
        let mut store = KvStore::new();
        store.upsert("short", "a").unwrap();
        store.upsert("medium", "abcde").unwrap();
        store.upsert("long", "abcdefghij").unwrap();

        let longest = store.longest_values(2);
        assert_eq!(longest.len(), 2);
        assert_eq!(longest[0].0, "long");
        assert_eq!(longest[1].0, "medium");
    }
}