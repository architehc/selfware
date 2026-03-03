//! Key-Value Store Module
//!
//! Provides a persistent key-value store with tagging and metadata support.
//!
//! # Features
//! - Simple key-value storage with persistence
//! - Tag-based queries
//! - Metadata filtering
//! - Key pattern matching
//! - Serialization utilities
//!
//! # Example
//!
//! ```ignore
//! use selfware::kv_store::{KvStore, Entry};
//!
//! // Create a new in-memory store
//! let mut store = KvStore::new();
//! store.insert("key1", "value1").unwrap();
//!
//! // Create a persistent store
//! let mut store = KvStore::with_path("data/store.json").unwrap();
//! store.upsert("key2", "value2").unwrap();
//!
//! // Query by tag
//! let entry = Entry::new("key3", "value3")
//!     .with_tags(vec!["important".to_string()]);
//! store.upsert("key3", entry).unwrap();
//! let important = store.by_tag("important");
//!
//! // Serialize store
//! let store_data: HashMap<String, Entry> = store.entries()
//!     .map(|e| (e.key.clone(), e.clone()))
//!     .collect();
//! let json = store_data.serialize_to_json().unwrap();
//! ```

mod entry;
mod query;
mod serialization;
mod store;

// Re-export the main types for convenience
pub use entry::Entry;
pub use serialization::{StoreSerializer, SerializedEntry};
pub use store::{KvStore, KvStoreError, Result};