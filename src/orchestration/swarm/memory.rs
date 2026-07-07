//! Swarm Shared Memory
//!
//! Shared working memory for agent coordination.

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::time::SystemTime;

/// Maximum number of entries retained in the access log before older entries
/// are evicted. This prevents unbounded memory growth when many read/write/delete
/// operations are performed on shared memory.
const MAX_ACCESS_LOG_ENTRIES: usize = 10_000;

/// Shared working memory for the swarm
#[derive(Debug, Clone, Default)]
pub struct SharedMemory {
    /// Key-value store
    pub(super) data: HashMap<String, MemoryEntry>,
    /// Access log (bounded ring buffer)
    pub(super) access_log: VecDeque<MemoryAccess>,
}

/// Memory entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryEntry {
    /// Key
    pub key: String,
    /// Value
    pub value: String,
    /// Created by agent
    pub created_by: String,
    /// Created timestamp
    pub created_at: u64,
    /// Last modified by
    pub modified_by: Option<String>,
    /// Last modified timestamp
    pub modified_at: Option<u64>,
    /// Access count
    pub access_count: u32,
    /// Tags
    pub tags: Vec<String>,
}

/// Memory access record
#[derive(Debug, Clone)]
pub struct MemoryAccess {
    pub key: String,
    pub agent_id: String,
    pub action: MemoryAction,
    pub timestamp: u64,
}

/// Memory action type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryAction {
    Read,
    Write,
    Delete,
}

impl SharedMemory {
    /// Create new shared memory
    pub fn new() -> Self {
        Self::default()
    }

    /// Write a value
    pub fn write(
        &mut self,
        key: impl Into<String>,
        value: impl Into<String>,
        agent_id: impl Into<String>,
    ) {
        let key = key.into();
        let agent_id = agent_id.into();
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        if let Some(entry) = self.data.get_mut(&key) {
            entry.value = value.into();
            entry.modified_by = Some(agent_id.clone());
            entry.modified_at = Some(now);
        } else {
            self.data.insert(
                key.clone(),
                MemoryEntry {
                    key: key.clone(),
                    value: value.into(),
                    created_by: agent_id.clone(),
                    created_at: now,
                    modified_by: None,
                    modified_at: None,
                    access_count: 0,
                    tags: Vec::new(),
                },
            );
        }

        self.access_log.push_back(MemoryAccess {
            key,
            agent_id,
            action: MemoryAction::Write,
            timestamp: now,
        });
        if self.access_log.len() > MAX_ACCESS_LOG_ENTRIES {
            self.access_log.pop_front();
        }
    }

    /// Read a value
    pub fn read(&mut self, key: &str, agent_id: impl Into<String>) -> Option<String> {
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        if let Some(entry) = self.data.get_mut(key) {
            entry.access_count += 1;

            self.access_log.push_back(MemoryAccess {
                key: key.to_string(),
                agent_id: agent_id.into(),
                action: MemoryAction::Read,
                timestamp: now,
            });
            if self.access_log.len() > MAX_ACCESS_LOG_ENTRIES {
                self.access_log.pop_front();
            }

            Some(entry.value.clone())
        } else {
            None
        }
    }

    /// Read without tracking
    pub fn peek(&self, key: &str) -> Option<&str> {
        self.data.get(key).map(|e| e.value.as_str())
    }

    /// Delete a value
    pub fn delete(&mut self, key: &str, agent_id: impl Into<String>) -> Option<String> {
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        self.access_log.push_back(MemoryAccess {
            key: key.to_string(),
            agent_id: agent_id.into(),
            action: MemoryAction::Delete,
            timestamp: now,
        });
        if self.access_log.len() > MAX_ACCESS_LOG_ENTRIES {
            self.access_log.pop_front();
        }

        self.data.remove(key).map(|e| e.value)
    }

    /// List all keys
    pub fn keys(&self) -> Vec<&str> {
        self.data.keys().map(|k| k.as_str()).collect()
    }

    /// Get all entries
    pub fn entries(&self) -> Vec<&MemoryEntry> {
        self.data.values().collect()
    }

    /// Tag an entry
    pub fn tag(&mut self, key: &str, tag: impl Into<String>) {
        if let Some(entry) = self.data.get_mut(key) {
            entry.tags.push(tag.into());
        }
    }

    /// Find by tag
    pub fn find_by_tag(&self, tag: &str) -> Vec<&MemoryEntry> {
        self.data
            .values()
            .filter(|e| e.tags.contains(&tag.to_string()))
            .collect()
    }

    /// Get access log
    pub fn access_log(&self) -> &VecDeque<MemoryAccess> {
        &self.access_log
    }

    /// Clear memory
    pub fn clear(&mut self) {
        self.data.clear();
        self.access_log.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── new() / Default ──────────────────────────────────────────

    #[test]
    fn new_creates_empty_memory() {
        let mem = SharedMemory::new();
        assert!(mem.keys().is_empty());
        assert!(mem.entries().is_empty());
        assert!(mem.access_log().is_empty());
    }

    #[test]
    fn default_equals_new() {
        let a = SharedMemory::new();
        let b = SharedMemory::default();
        assert_eq!(a.keys(), b.keys());
        assert_eq!(a.entries().len(), b.entries().len());
    }

    // ── write() ──────────────────────────────────────────────────

    #[test]
    fn write_creates_new_entry() {
        let mut mem = SharedMemory::new();
        mem.write("key1", "value1", "agent-a");

        assert_eq!(mem.peek("key1"), Some("value1"));
        assert_eq!(mem.keys(), vec!["key1"]);
        assert_eq!(mem.entries().len(), 1);
    }

    #[test]
    fn write_records_access_log() {
        let mut mem = SharedMemory::new();
        mem.write("k", "v", "agent-x");

        let log = mem.access_log();
        assert_eq!(log.len(), 1);
        assert_eq!(log[0].key, "k");
        assert_eq!(log[0].agent_id, "agent-x");
        assert_eq!(log[0].action, MemoryAction::Write);
    }

    #[test]
    fn write_updates_existing_entry() {
        let mut mem = SharedMemory::new();
        mem.write("key", "old", "agent-a");
        mem.write("key", "new", "agent-b");

        assert_eq!(mem.peek("key"), Some("new"));
        assert_eq!(mem.entries().len(), 1); // no duplicate

        let entry = mem.entries().into_iter().next().unwrap();
        assert_eq!(entry.created_by, "agent-a");
        assert_eq!(entry.modified_by.as_deref(), Some("agent-b"));
        assert!(entry.modified_at.is_some());
    }

    #[test]
    fn write_new_entry_has_no_modified_fields() {
        let mut mem = SharedMemory::new();
        mem.write("key", "val", "agent-a");

        let entry = &mem.data["key"];
        assert!(entry.modified_by.is_none());
        assert!(entry.modified_at.is_none());
        assert_eq!(entry.access_count, 0);
    }

    #[test]
    fn write_preserves_access_count_on_update() {
        let mut mem = SharedMemory::new();
        mem.write("key", "v1", "a");
        // Read twice to bump access_count
        let _ = mem.read("key", "reader");
        let _ = mem.read("key", "reader");
        // Now overwrite
        mem.write("key", "v2", "writer");

        let entry = &mem.data["key"];
        assert_eq!(entry.value, "v2");
        assert_eq!(entry.access_count, 2);
    }

    #[test]
    fn write_preserves_tags_on_update() {
        let mut mem = SharedMemory::new();
        mem.write("key", "v1", "a");
        mem.tag("key", "important");
        mem.write("key", "v2", "b");

        let entry = &mem.data["key"];
        assert!(entry.tags.contains(&"important".to_string()));
    }

    #[test]
    fn write_accepts_string_and_str() {
        let mut mem = SharedMemory::new();
        let key = String::from("k");
        let val = String::from("v");
        let agent = String::from("a");
        mem.write(key.as_str(), val.as_str(), agent.as_str());
        assert_eq!(mem.peek("k"), Some("v"));
    }

    // ── read() ───────────────────────────────────────────────────

    #[test]
    fn read_returns_value_for_existing_key() {
        let mut mem = SharedMemory::new();
        mem.write("key", "value", "a");
        assert_eq!(mem.read("key", "reader"), Some("value".to_string()));
    }

    #[test]
    fn read_returns_none_for_missing_key() {
        let mut mem = SharedMemory::new();
        assert_eq!(mem.read("missing", "reader"), None);
    }

    #[test]
    fn read_increments_access_count() {
        let mut mem = SharedMemory::new();
        mem.write("key", "v", "a");

        let _ = mem.read("key", "r1");
        assert_eq!(mem.data["key"].access_count, 1);

        let _ = mem.read("key", "r2");
        assert_eq!(mem.data["key"].access_count, 2);
    }

    #[test]
    fn read_logs_access() {
        let mut mem = SharedMemory::new();
        mem.write("key", "v", "writer");
        let _ = mem.read("key", "reader");

        let log = mem.access_log();
        assert_eq!(log.len(), 2);
        assert_eq!(log[1].key, "key");
        assert_eq!(log[1].agent_id, "reader");
        assert_eq!(log[1].action, MemoryAction::Read);
    }

    #[test]
    fn read_missing_key_does_not_log() {
        let mut mem = SharedMemory::new();
        let _ = mem.read("nope", "reader");
        assert!(mem.access_log().is_empty());
    }

    // ── peek() ───────────────────────────────────────────────────

    #[test]
    fn peek_returns_value_without_tracking() {
        let mut mem = SharedMemory::new();
        mem.write("key", "v", "a");
        assert_eq!(mem.peek("key"), Some("v"));
        // access_count should remain 0
        assert_eq!(mem.data["key"].access_count, 0);
        // access_log should only have the write entry
        assert_eq!(mem.access_log().len(), 1);
    }

    #[test]
    fn peek_returns_none_for_missing_key() {
        let mem = SharedMemory::new();
        assert_eq!(mem.peek("missing"), None);
    }

    // ── delete() ─────────────────────────────────────────────────

    #[test]
    fn delete_removes_entry_and_returns_value() {
        let mut mem = SharedMemory::new();
        mem.write("key", "value", "a");
        let deleted = mem.delete("key", "deleter");
        assert_eq!(deleted, Some("value".to_string()));
        assert_eq!(mem.peek("key"), None);
    }

    #[test]
    fn delete_returns_none_for_missing_key() {
        let mut mem = SharedMemory::new();
        assert_eq!(mem.delete("missing", "d"), None);
    }

    #[test]
    fn delete_logs_action_even_if_key_missing() {
        let mut mem = SharedMemory::new();
        let _ = mem.delete("ghost", "deleter");
        let log = mem.access_log();
        assert_eq!(log.len(), 1);
        assert_eq!(log[0].action, MemoryAction::Delete);
        assert_eq!(log[0].key, "ghost");
    }

    #[test]
    fn delete_then_peek_returns_none() {
        let mut mem = SharedMemory::new();
        mem.write("k", "v", "a");
        let _ = mem.delete("k", "d");
        assert!(mem.peek("k").is_none());
        assert_eq!(mem.keys().len(), 0);
    }

    // ── keys() ───────────────────────────────────────────────────

    #[test]
    fn keys_returns_all_keys() {
        let mut mem = SharedMemory::new();
        mem.write("a", "1", "x");
        mem.write("b", "2", "x");
        mem.write("c", "3", "x");

        let mut keys = mem.keys();
        keys.sort();
        assert_eq!(keys, vec!["a", "b", "c"]);
    }

    #[test]
    fn keys_empty_when_no_data() {
        let mem = SharedMemory::new();
        assert!(mem.keys().is_empty());
    }

    #[test]
    fn keys_reflects_deletions() {
        let mut mem = SharedMemory::new();
        mem.write("a", "1", "x");
        mem.write("b", "2", "x");
        let _ = mem.delete("a", "d");
        assert_eq!(mem.keys(), vec!["b"]);
    }

    // ── entries() ────────────────────────────────────────────────

    #[test]
    fn entries_returns_all_entries() {
        let mut mem = SharedMemory::new();
        mem.write("a", "1", "x");
        mem.write("b", "2", "x");

        let entries = mem.entries();
        assert_eq!(entries.len(), 2);
        let values: Vec<&str> = entries.iter().map(|e| e.value.as_str()).collect();
        assert!(values.contains(&"1"));
        assert!(values.contains(&"2"));
    }

    #[test]
    fn entries_empty_when_no_data() {
        let mem = SharedMemory::new();
        assert!(mem.entries().is_empty());
    }

    // ── tag() ────────────────────────────────────────────────────

    #[test]
    fn tag_adds_tag_to_existing_entry() {
        let mut mem = SharedMemory::new();
        mem.write("key", "v", "a");
        mem.tag("key", "important");

        assert!(mem.data["key"].tags.contains(&"important".to_string()));
    }

    #[test]
    fn tag_multiple_tags_on_same_entry() {
        let mut mem = SharedMemory::new();
        mem.write("key", "v", "a");
        mem.tag("key", "t1");
        mem.tag("key", "t2");
        mem.tag("key", "t3");

        assert_eq!(mem.data["key"].tags.len(), 3);
    }

    #[test]
    fn tag_on_missing_key_is_noop() {
        let mut mem = SharedMemory::new();
        mem.tag("nonexistent", "tag");
        // Should not panic, no entries created
        assert!(mem.keys().is_empty());
    }

    // ── find_by_tag() ────────────────────────────────────────────

    #[test]
    fn find_by_tag_returns_matching_entries() {
        let mut mem = SharedMemory::new();
        mem.write("a", "1", "x");
        mem.write("b", "2", "x");
        mem.write("c", "3", "x");
        mem.tag("a", "important");
        mem.tag("c", "important");

        let results = mem.find_by_tag("important");
        assert_eq!(results.len(), 2);
        let keys: Vec<&str> = results.iter().map(|e| e.key.as_str()).collect();
        assert!(keys.contains(&"a"));
        assert!(keys.contains(&"c"));
    }

    #[test]
    fn find_by_tag_returns_empty_when_no_match() {
        let mut mem = SharedMemory::new();
        mem.write("a", "1", "x");
        assert!(mem.find_by_tag("nonexistent-tag").is_empty());
    }

    #[test]
    fn find_by_tag_empty_when_no_entries() {
        let mem = SharedMemory::new();
        assert!(mem.find_by_tag("anything").is_empty());
    }

    // ── access_log() ─────────────────────────────────────────────

    #[test]
    fn access_log_tracks_all_operations_in_order() {
        let mut mem = SharedMemory::new();
        mem.write("k", "v1", "w");
        let _ = mem.read("k", "r");
        let _ = mem.delete("k", "d");

        let log = mem.access_log();
        assert_eq!(log.len(), 3);
        assert_eq!(log[0].action, MemoryAction::Write);
        assert_eq!(log[1].action, MemoryAction::Read);
        assert_eq!(log[2].action, MemoryAction::Delete);
    }

    #[test]
    fn access_log_empty_for_new_memory() {
        let mem = SharedMemory::new();
        assert!(mem.access_log().is_empty());
    }

    // ── clear() ──────────────────────────────────────────────────

    #[test]
    fn clear_removes_all_data_and_log() {
        let mut mem = SharedMemory::new();
        mem.write("a", "1", "x");
        mem.write("b", "2", "x");
        let _ = mem.read("a", "r");

        mem.clear();
        assert!(mem.keys().is_empty());
        assert!(mem.entries().is_empty());
        assert!(mem.access_log().is_empty());
    }

    #[test]
    fn clear_on_empty_memory_is_noop() {
        let mut mem = SharedMemory::new();
        mem.clear();
        assert!(mem.keys().is_empty());
    }

    #[test]
    fn clear_allows_reuse() {
        let mut mem = SharedMemory::new();
        mem.write("a", "1", "x");
        mem.clear();
        mem.write("b", "2", "y");
        assert_eq!(mem.peek("b"), Some("2"));
        assert_eq!(mem.keys(), vec!["b"]);
    }

    // ── Access log bounding (MAX_ACCESS_LOG_ENTRIES) ─────────────

    #[test]
    fn access_log_is_bounded() {
        let mut mem = SharedMemory::new();
        // Generate more than MAX_ACCESS_LOG_ENTRIES writes
        for i in 0..(MAX_ACCESS_LOG_ENTRIES + 100) {
            mem.write(format!("key{}", i), "v", "a");
        }
        assert!(
            mem.access_log().len() <= MAX_ACCESS_LOG_ENTRIES,
            "access log grew to {}",
            mem.access_log().len()
        );
        // The oldest entries should have been evicted
        assert!(
            mem.access_log().len() >= MAX_ACCESS_LOG_ENTRIES - 1,
            "access log too small: {}",
            mem.access_log().len()
        );
    }

    // ── MemoryEntry fields ───────────────────────────────────────

    #[test]
    fn memory_entry_has_correct_fields_on_creation() {
        let mut mem = SharedMemory::new();
        mem.write("k", "v", "creator");

        let entry = &mem.data["k"];
        assert_eq!(entry.key, "k");
        assert_eq!(entry.value, "v");
        assert_eq!(entry.created_by, "creator");
        assert!(entry.created_at > 0);
        assert!(entry.modified_by.is_none());
        assert!(entry.modified_at.is_none());
        assert_eq!(entry.access_count, 0);
        assert!(entry.tags.is_empty());
    }

    // ── MemoryAction variants ────────────────────────────────────

    #[test]
    fn memory_action_variants_are_distinct() {
        assert_ne!(MemoryAction::Read, MemoryAction::Write);
        assert_ne!(MemoryAction::Write, MemoryAction::Delete);
        assert_ne!(MemoryAction::Read, MemoryAction::Delete);
    }

    // ── Integration: multi-agent scenario ────────────────────────

    #[test]
    fn multi_agent_write_read_delete_scenario() {
        let mut mem = SharedMemory::new();

        // Agent 1 writes a task
        mem.write("task/result", "pending", "agent-1");

        // Agent 2 reads it
        let val = mem.read("task/result", "agent-2");
        assert_eq!(val.as_deref(), Some("pending"));

        // Agent 3 updates it
        mem.write("task/result", "completed", "agent-3");

        // Agent 1 reads the update
        let val = mem.read("task/result", "agent-1");
        assert_eq!(val.as_deref(), Some("completed"));

        // Verify entry metadata
        let entry = &mem.data["task/result"];
        assert_eq!(entry.created_by, "agent-1");
        assert_eq!(entry.modified_by.as_deref(), Some("agent-3"));
        assert_eq!(entry.access_count, 2);

        // Agent 2 deletes it
        let deleted = mem.delete("task/result", "agent-2");
        assert_eq!(deleted.as_deref(), Some("completed"));

        // Verify it's gone
        assert!(mem.peek("task/result").is_none());

        // Verify full access log
        let log = mem.access_log();
        assert_eq!(log.len(), 5); // write, read, write(update), read, delete
        assert_eq!(log[0].agent_id, "agent-1");
        assert_eq!(log[1].agent_id, "agent-2");
        assert_eq!(log[2].agent_id, "agent-3");
        assert_eq!(log[3].agent_id, "agent-1");
        assert_eq!(log[4].agent_id, "agent-2");
    }

    #[test]
    fn tag_survives_value_update() {
        let mut mem = SharedMemory::new();
        mem.write("config", "v1", "a");
        mem.tag("config", "persistent");
        mem.write("config", "v2", "b");

        let tagged = mem.find_by_tag("persistent");
        assert_eq!(tagged.len(), 1);
        assert_eq!(tagged[0].value, "v2");
    }

    #[test]
    fn overwrite_does_not_create_duplicate_keys() {
        let mut mem = SharedMemory::new();
        mem.write("k", "v1", "a");
        mem.write("k", "v2", "a");
        mem.write("k", "v3", "a");

        assert_eq!(mem.keys().len(), 1);
        assert_eq!(mem.entries().len(), 1);
        assert_eq!(mem.peek("k"), Some("v3"));
    }
}
