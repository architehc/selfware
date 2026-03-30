//! SWL Workflow State Management
//!
//! Provides persistent state storage for workflows with multiple backend options
//! including file-based, in-memory, and Redis backends.

pub mod backend;
pub mod validation;

use crate::errors::Result;
use crate::swl::types::schema::StateSchema;
use std::collections::HashMap;
use std::path::PathBuf;
use tracing::{debug, info, warn};

pub use backend::{StateBackend, StateBackendType};

/// Workflow state manager
pub struct StateManager {
    /// The backend used for state persistence
    backend: Box<dyn StateBackend>,
    /// Optional schema for validation
    schema: Option<StateSchema>,
    /// Workflow name (used for state file naming)
    workflow_name: String,
    /// Whether to auto-save after each modification
    auto_save: bool,
    /// In-memory cache of current state
    cache: HashMap<String, serde_json::Value>,
    /// Dirty flag to track unsaved changes
    dirty: bool,
}

impl std::fmt::Debug for StateManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StateManager")
            .field("workflow_name", &self.workflow_name)
            .field("schema", &self.schema)
            .field("auto_save", &self.auto_save)
            .field("cache", &self.cache)
            .field("dirty", &self.dirty)
            .field("backend", &"<dyn StateBackend>")
            .finish()
    }
}

impl StateManager {
    /// Create a new state manager with file backend (default)
    pub fn new_file_based(workflow_name: &str, base_dir: PathBuf) -> Result<Self> {
        let backend = backend::FileBackend::new(base_dir)?;
        Ok(Self {
            backend: Box::new(backend),
            schema: None,
            workflow_name: workflow_name.to_string(),
            auto_save: true,
            cache: HashMap::new(),
            dirty: false,
        })
    }

    /// Create a new state manager with memory backend
    pub fn new_memory(workflow_name: &str) -> Self {
        Self {
            backend: Box::new(backend::MemoryBackend::new()),
            schema: None,
            workflow_name: workflow_name.to_string(),
            auto_save: false,
            cache: HashMap::new(),
            dirty: false,
        }
    }

    /// Create a new state manager with Redis backend
    #[cfg(feature = "redis")]
    pub async fn new_redis(workflow_name: &str, url: &str) -> Result<Self> {
        let backend = backend::RedisBackend::new(url, workflow_name).await?;
        Ok(Self {
            backend: Box::new(backend),
            schema: None,
            workflow_name: workflow_name.to_string(),
            auto_save: true,
            cache: HashMap::new(),
            dirty: false,
        })
    }

    /// Create a state manager from backend type
    pub async fn from_backend_type(
        backend_type: StateBackendType,
        workflow_name: &str,
    ) -> Result<Self> {
        #[cfg(feature = "redis")]
        match backend_type {
            StateBackendType::File { base_dir } => Self::new_file_based(workflow_name, base_dir),
            StateBackendType::Memory => Ok(Self::new_memory(workflow_name)),
            StateBackendType::Redis { url } => Self::new_redis(workflow_name, &url).await,
        }

        #[cfg(not(feature = "redis"))]
        match backend_type {
            StateBackendType::File { base_dir } => Self::new_file_based(workflow_name, base_dir),
            StateBackendType::Memory => Ok(Self::new_memory(workflow_name)),
        }
    }

    /// Set the state schema for validation
    pub fn with_schema(mut self, schema: StateSchema) -> Self {
        self.schema = Some(schema);
        self
    }

    /// Set auto-save behavior
    pub fn with_auto_save(mut self, auto_save: bool) -> Self {
        self.auto_save = auto_save;
        self
    }

    /// Load state from the backend
    pub async fn load(&mut self) -> Result<()> {
        debug!("Loading state for workflow: {}", self.workflow_name);

        let state = self.backend.load(&self.workflow_name).await?;

        // Validate against schema if present
        if let Some(ref schema) = self.schema {
            if let Err(e) = validation::validate_state_against_schema(&state, schema) {
                warn!("State validation failed on load: {}", e);
                // Continue with defaults applied
            }
        }

        self.cache = state;
        self.dirty = false;

        info!(
            "Loaded state for '{}' with {} fields",
            self.workflow_name,
            self.cache.len()
        );
        Ok(())
    }

    /// Save state to the backend
    pub async fn save(&mut self) -> Result<()> {
        if !self.dirty {
            debug!("State unchanged, skipping save");
            return Ok(());
        }

        debug!("Saving state for workflow: {}", self.workflow_name);

        // Validate before saving if schema is present
        if let Some(ref schema) = self.schema {
            validation::validate_state_against_schema(&self.cache, schema)?;
        }

        self.backend.save(&self.workflow_name, &self.cache).await?;
        self.dirty = false;

        info!(
            "Saved state for '{}' with {} fields",
            self.workflow_name,
            self.cache.len()
        );
        Ok(())
    }

    /// Get a state value
    pub fn get(&self, key: &str) -> Option<&serde_json::Value> {
        self.cache.get(key)
    }

    /// Set a state value
    pub fn set(&mut self, key: String, value: serde_json::Value) -> Result<()> {
        // Validate the value if schema is present
        if let Some(ref schema) = self.schema {
            if let Some(field) = schema.get_field(&key) {
                validation::validate_value_type(&value, &field.field_type).map_err(|e| {
                    crate::errors::SelfwareError::Internal(format!(
                        "Validation failed for field '{}': {}",
                        key, e
                    ))
                })?;
            }
        }

        self.cache.insert(key, value);
        self.dirty = true;

        if self.auto_save {
            // Note: This would need to be called in an async context
            // For now, we mark dirty and let the runtime handle saves
        }

        Ok(())
    }

    /// Delete a state value
    pub fn delete(&mut self, key: &str) -> bool {
        let removed = self.cache.remove(key).is_some();
        if removed {
            self.dirty = true;
        }
        removed
    }

    /// Check if a key exists
    pub fn has(&self, key: &str) -> bool {
        self.cache.contains_key(key)
    }

    /// Get all state keys
    pub fn keys(&self) -> Vec<&String> {
        self.cache.keys().collect()
    }

    /// Get the entire state as a HashMap
    pub fn get_all(&self) -> &HashMap<String, serde_json::Value> {
        &self.cache
    }

    /// Set multiple values at once
    pub fn set_all(&mut self, values: HashMap<String, serde_json::Value>) -> Result<()> {
        for (key, value) in values {
            self.set(key, value)?;
        }
        Ok(())
    }

    /// Clear all state
    pub fn clear(&mut self) {
        if !self.cache.is_empty() {
            self.cache.clear();
            self.dirty = true;
        }
    }

    /// Check if state needs to be saved
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// Get the workflow name
    pub fn workflow_name(&self) -> &str {
        &self.workflow_name
    }

    /// Apply default values from schema
    pub fn apply_defaults(&mut self) {
        if let Some(ref schema) = self.schema {
            for field in &schema.fields {
                if !self.cache.contains_key(&field.name) {
                    if let Some(ref default) = field.default {
                        // Convert YAML value to JSON value
                        if let Ok(json_value) = serde_json::to_value(default) {
                            self.cache.insert(field.name.clone(), json_value);
                            self.dirty = true;
                        }
                    }
                }
            }
        }
    }

    /// Get state size in bytes (approximate)
    pub fn size_bytes(&self) -> usize {
        serde_json::to_vec(&self.cache)
            .map(|v| v.len())
            .unwrap_or(0)
    }

    /// Export state as JSON string
    pub fn export_json(&self) -> Result<String> {
        serde_json::to_string_pretty(&self.cache).map_err(|e| {
            crate::errors::SelfwareError::Internal(format!("Failed to serialize state: {}", e))
        })
    }

    /// Import state from JSON string
    pub fn import_json(&mut self, json: &str) -> Result<()> {
        let state: HashMap<String, serde_json::Value> =
            serde_json::from_str(json).map_err(|e| {
                crate::errors::SelfwareError::Internal(format!("Failed to parse state JSON: {}", e))
            })?;

        // Validate against schema if present
        if let Some(ref schema) = self.schema {
            validation::validate_state_against_schema(&state, schema)?;
        }

        self.cache = state;
        self.dirty = true;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::swl::types::schema::{FieldType, StateField};

    fn create_test_schema() -> StateSchema {
        StateSchema {
            fields: vec![
                StateField {
                    name: "name".to_string(),
                    field_type: FieldType::String,
                    default: Some(serde_yaml::Value::String("default_name".to_string())),
                    description: None,
                },
                StateField {
                    name: "count".to_string(),
                    field_type: FieldType::Integer,
                    default: Some(serde_yaml::Value::Number(0.into())),
                    description: None,
                },
            ],
        }
    }

    #[test]
    fn test_state_manager_memory() {
        let mut manager = StateManager::new_memory("test_workflow");

        // Set values
        manager
            .set("key1".to_string(), serde_json::json!("value1"))
            .unwrap();
        manager
            .set("key2".to_string(), serde_json::json!(42))
            .unwrap();

        // Check values
        assert_eq!(manager.get("key1"), Some(&serde_json::json!("value1")));
        assert_eq!(manager.get("key2"), Some(&serde_json::json!(42)));
        assert!(manager.get("missing").is_none());

        // Check dirty flag
        assert!(manager.is_dirty());

        // Delete value
        assert!(manager.delete("key1"));
        assert!(!manager.delete("missing"));

        // Clear
        manager.clear();
        assert!(manager.get_all().is_empty());
    }

    #[test]
    fn test_state_manager_with_schema() {
        let schema = create_test_schema();
        let mut manager = StateManager::new_memory("test_workflow").with_schema(schema);

        // Apply defaults
        manager.apply_defaults();

        // Check defaults were applied
        assert_eq!(
            manager.get("name"),
            Some(&serde_json::json!("default_name"))
        );
        assert_eq!(manager.get("count"), Some(&serde_json::json!(0)));
    }

    #[test]
    fn test_state_manager_export_import() {
        let mut manager = StateManager::new_memory("test_workflow");

        manager
            .set("key1".to_string(), serde_json::json!("value1"))
            .unwrap();
        manager
            .set("key2".to_string(), serde_json::json!(42))
            .unwrap();

        // Export
        let json = manager.export_json().unwrap();
        assert!(json.contains("key1"));
        assert!(json.contains("value1"));

        // Clear and import
        manager.clear();
        manager.import_json(&json).unwrap();

        assert_eq!(manager.get("key1"), Some(&serde_json::json!("value1")));
        assert_eq!(manager.get("key2"), Some(&serde_json::json!(42)));
    }

    #[tokio::test]
    async fn test_state_manager_file_backend() {
        let temp_dir = tempfile::tempdir().unwrap();
        let mut manager =
            StateManager::new_file_based("test_workflow", temp_dir.path().to_path_buf()).unwrap();

        manager
            .set("key1".to_string(), serde_json::json!("value1"))
            .unwrap();
        manager.save().await.unwrap();

        // Create new manager and load
        let mut manager2 =
            StateManager::new_file_based("test_workflow", temp_dir.path().to_path_buf()).unwrap();
        manager2.load().await.unwrap();

        assert_eq!(manager2.get("key1"), Some(&serde_json::json!("value1")));
    }
}
