//! State Backend Implementations
//!
//! Provides multiple storage backends for workflow state:
//! - FileBackend: Persistent JSON file storage
//! - MemoryBackend: In-memory storage (non-persistent)
//! - RedisBackend: Redis-based distributed storage (requires 'redis' feature)

use crate::errors::{Result, SelfwareError};
use async_trait::async_trait;
use serde_json::Value;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tracing::{debug, error};

/// Backend type configuration
#[derive(Debug, Clone)]
pub enum StateBackendType {
    /// File-based storage at the specified directory
    File { base_dir: PathBuf },
    /// In-memory storage (non-persistent)
    Memory,
    /// Redis-based storage
    #[cfg(feature = "redis")]
    Redis { url: String },
}

impl Default for StateBackendType {
    fn default() -> Self {
        StateBackendType::File {
            base_dir: Self::default_state_dir(),
        }
    }
}

impl StateBackendType {
    /// Get the default state directory (.selfware/state)
    pub fn default_state_dir() -> PathBuf {
        dirs::home_dir()
            .map(|h| h.join(".selfware").join("state"))
            .unwrap_or_else(|| PathBuf::from(".selfware/state"))
    }

    /// From string identifier
    pub fn parse_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "memory" => Some(StateBackendType::Memory),
            "file" => Some(StateBackendType::File {
                base_dir: Self::default_state_dir(),
            }),
            _ if s.starts_with("redis://") => {
                #[cfg(feature = "redis")]
                {
                    Some(StateBackendType::Redis { url: s.to_string() })
                }
                #[cfg(not(feature = "redis"))]
                {
                    None
                }
            }
            _ => None,
        }
    }
}

/// Trait for state storage backends
#[async_trait]
pub trait StateBackend: Send + Sync {
    /// Load state for a workflow
    async fn load(&self, workflow_name: &str) -> Result<HashMap<String, Value>>;

    /// Save state for a workflow
    async fn save(&self, workflow_name: &str, state: &HashMap<String, Value>) -> Result<()>;

    /// Delete state for a workflow
    async fn delete(&self, workflow_name: &str) -> Result<()>;

    /// Check if state exists for a workflow
    async fn exists(&self, workflow_name: &str) -> bool;

    /// List all workflows with saved state
    async fn list(&self) -> Result<Vec<String>>;
}

/// File-based state backend
pub struct FileBackend {
    /// Base directory for state files
    base_dir: PathBuf,
}

impl FileBackend {
    /// Create a new file backend with the specified base directory
    pub fn new(base_dir: PathBuf) -> Result<Self> {
        // Ensure the directory exists
        std::fs::create_dir_all(&base_dir).map_err(|e| {
            SelfwareError::Internal(format!(
                "Failed to create state directory '{}': {}",
                base_dir.display(),
                e
            ))
        })?;

        Ok(Self { base_dir })
    }

    /// Get the file path for a workflow
    fn state_file_path(&self, workflow_name: &str) -> PathBuf {
        self.base_dir.join(format!("{}.json", workflow_name))
    }
}

#[async_trait]
impl StateBackend for FileBackend {
    async fn load(&self, workflow_name: &str) -> Result<HashMap<String, Value>> {
        let path = self.state_file_path(workflow_name);

        if !path.exists() {
            debug!("No existing state file for workflow: {}", workflow_name);
            return Ok(HashMap::new());
        }

        let content = tokio::fs::read_to_string(&path).await.map_err(|e| {
            SelfwareError::Internal(format!(
                "Failed to read state file '{}': {}",
                path.display(),
                e
            ))
        })?;

        if content.trim().is_empty() {
            return Ok(HashMap::new());
        }

        let state: HashMap<String, Value> = serde_json::from_str(&content).map_err(|e| {
            SelfwareError::Internal(format!(
                "Failed to parse state file '{}': {}",
                path.display(),
                e
            ))
        })?;

        debug!(
            "Loaded state for '{}' from {}",
            workflow_name,
            path.display()
        );
        Ok(state)
    }

    async fn save(&self, workflow_name: &str, state: &HashMap<String, Value>) -> Result<()> {
        let path = self.state_file_path(workflow_name);
        let temp_path = path.with_extension("tmp");

        // Write to temp file first for atomicity
        let content = serde_json::to_string_pretty(state)
            .map_err(|e| SelfwareError::Internal(format!("Failed to serialize state: {}", e)))?;

        tokio::fs::write(&temp_path, content).await.map_err(|e| {
            SelfwareError::Internal(format!(
                "Failed to write state temp file '{}': {}",
                temp_path.display(),
                e
            ))
        })?;

        // Atomic rename
        tokio::fs::rename(&temp_path, &path).await.map_err(|e| {
            SelfwareError::Internal(format!(
                "Failed to rename state file to '{}': {}",
                path.display(),
                e
            ))
        })?;

        debug!("Saved state for '{}' to {}", workflow_name, path.display());
        Ok(())
    }

    async fn delete(&self, workflow_name: &str) -> Result<()> {
        let path = self.state_file_path(workflow_name);

        if path.exists() {
            tokio::fs::remove_file(&path).await.map_err(|e| {
                SelfwareError::Internal(format!(
                    "Failed to delete state file '{}': {}",
                    path.display(),
                    e
                ))
            })?;
            debug!(
                "Deleted state for '{}' from {}",
                workflow_name,
                path.display()
            );
        }

        Ok(())
    }

    async fn exists(&self, workflow_name: &str) -> bool {
        self.state_file_path(workflow_name).exists()
    }

    async fn list(&self) -> Result<Vec<String>> {
        let mut entries = tokio::fs::read_dir(&self.base_dir).await.map_err(|e| {
            SelfwareError::Internal(format!(
                "Failed to read state directory '{}': {}",
                self.base_dir.display(),
                e
            ))
        })?;

        let mut workflows = Vec::new();

        while let Some(entry) = entries.next_entry().await.map_err(|e| {
            SelfwareError::Internal(format!("Failed to read directory entry: {}", e))
        })? {
            let path = entry.path();
            if path.extension().map(|e| e == "json").unwrap_or(false) {
                if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                    workflows.push(stem.to_string());
                }
            }
        }

        Ok(workflows)
    }
}

/// In-memory state backend (non-persistent)
pub struct MemoryBackend {
    /// Storage for all workflow states
    storage: std::sync::Mutex<HashMap<String, HashMap<String, Value>>>,
}

impl MemoryBackend {
    /// Create a new memory backend
    pub fn new() -> Self {
        Self {
            storage: std::sync::Mutex::new(HashMap::new()),
        }
    }
}

impl Default for MemoryBackend {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl StateBackend for MemoryBackend {
    async fn load(&self, workflow_name: &str) -> Result<HashMap<String, Value>> {
        let storage = self
            .storage
            .lock()
            .map_err(|_| SelfwareError::Internal("Memory backend lock poisoned".to_string()))?;

        Ok(storage.get(workflow_name).cloned().unwrap_or_default())
    }

    async fn save(&self, workflow_name: &str, state: &HashMap<String, Value>) -> Result<()> {
        let mut storage = self
            .storage
            .lock()
            .map_err(|_| SelfwareError::Internal("Memory backend lock poisoned".to_string()))?;

        storage.insert(workflow_name.to_string(), state.clone());
        debug!("Saved state for '{}' to memory", workflow_name);
        Ok(())
    }

    async fn delete(&self, workflow_name: &str) -> Result<()> {
        let mut storage = self
            .storage
            .lock()
            .map_err(|_| SelfwareError::Internal("Memory backend lock poisoned".to_string()))?;

        storage.remove(workflow_name);
        debug!("Deleted state for '{}' from memory", workflow_name);
        Ok(())
    }

    async fn exists(&self, workflow_name: &str) -> bool {
        let storage = match self.storage.lock() {
            Ok(s) => s,
            Err(_) => return false,
        };

        storage.contains_key(workflow_name)
    }

    async fn list(&self) -> Result<Vec<String>> {
        let storage = self
            .storage
            .lock()
            .map_err(|_| SelfwareError::Internal("Memory backend lock poisoned".to_string()))?;

        Ok(storage.keys().cloned().collect())
    }
}

/// Redis-based state backend (requires 'redis' feature)
#[cfg(feature = "redis")]
pub struct RedisBackend {
    /// Redis client
    client: redis::aio::MultiplexedConnection,
    /// Key prefix for this backend
    key_prefix: String,
}

#[cfg(feature = "redis")]
impl RedisBackend {
    /// Create a new Redis backend
    pub async fn new(url: &str, key_prefix: &str) -> Result<Self> {
        let client = redis::Client::open(url)
            .map_err(|e| SelfwareError::Internal(format!("Failed to connect to Redis: {}", e)))?;

        let conn = client
            .get_multiplexed_tokio_connection()
            .await
            .map_err(|e| {
                SelfwareError::Internal(format!("Failed to get Redis connection: {}", e))
            })?;

        Ok(Self {
            client: conn,
            key_prefix: format!("swl:state:{}", key_prefix),
        })
    }

    /// Get the full key for a workflow
    fn full_key(&self, workflow_name: &str) -> String {
        format!("{}:{}", self.key_prefix, workflow_name)
    }
}

#[cfg(feature = "redis")]
#[async_trait]
impl StateBackend for RedisBackend {
    async fn load(&self, workflow_name: &str) -> Result<HashMap<String, Value>> {
        let key = self.full_key(workflow_name);
        let mut conn = self.client.clone();

        let data: Option<String> = redis::cmd("GET")
            .arg(&key)
            .query_async(&mut conn)
            .await
            .map_err(|e| SelfwareError::Internal(format!("Redis GET failed: {}", e)))?;

        match data {
            Some(json) => {
                let state: HashMap<String, Value> = serde_json::from_str(&json).map_err(|e| {
                    SelfwareError::Internal(format!("Failed to parse Redis state: {}", e))
                })?;
                Ok(state)
            }
            None => Ok(HashMap::new()),
        }
    }

    async fn save(&self, workflow_name: &str, state: &HashMap<String, Value>) -> Result<()> {
        let key = self.full_key(workflow_name);
        let mut conn = self.client.clone();

        let json = serde_json::to_string(state)
            .map_err(|e| SelfwareError::Internal(format!("Failed to serialize state: {}", e)))?;

        redis::cmd("SET")
            .arg(&key)
            .arg(&json)
            .query_async::<()>(&mut conn)
            .await
            .map_err(|e| SelfwareError::Internal(format!("Redis SET failed: {}", e)))?;

        debug!("Saved state for '{}' to Redis", workflow_name);
        Ok(())
    }

    async fn delete(&self, workflow_name: &str) -> Result<()> {
        let key = self.full_key(workflow_name);
        let mut conn = self.client.clone();

        redis::cmd("DEL")
            .arg(&key)
            .query_async::<()>(&mut conn)
            .await
            .map_err(|e| SelfwareError::Internal(format!("Redis DEL failed: {}", e)))?;

        debug!("Deleted state for '{}' from Redis", workflow_name);
        Ok(())
    }

    async fn exists(&self, workflow_name: &str) -> bool {
        let key = self.full_key(workflow_name);
        let mut conn = self.client.clone();

        let exists: bool = redis::cmd("EXISTS")
            .arg(&key)
            .query_async(&mut conn)
            .await
            .unwrap_or(0)
            > 0;

        exists
    }

    async fn list(&self) -> Result<Vec<String>> {
        let pattern = format!("{}:*", self.key_prefix);
        let mut conn = self.client.clone();

        let keys: Vec<String> = redis::cmd("KEYS")
            .arg(&pattern)
            .query_async(&mut conn)
            .await
            .map_err(|e| SelfwareError::Internal(format!("Redis KEYS failed: {}", e)))?;

        let prefix_len = self.key_prefix.len() + 1; // +1 for the colon
        let workflows: Vec<String> = keys
            .into_iter()
            .filter_map(|k| {
                k.strip_prefix(&format!("{}:", self.key_prefix))
                    .map(|s| s.to_string())
            })
            .collect();

        Ok(workflows)
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/swl/state/backend/backend_test.rs"]
mod tests;
