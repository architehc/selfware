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
#[path = "../../../tests/unit/orchestration/swarm/memory/memory_test.rs"]
mod tests;
