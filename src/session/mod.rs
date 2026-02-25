//! Session and state management module
//!
//! This module contains session persistence and state management including:
//! - Checkpointing
//! - Caching
//! - Local-first storage
//! - Edit history

pub mod chat_store;
pub mod checkpoint;
pub mod edit_history;
pub mod encryption;
pub mod local_first;

#[cfg(feature = "cache")]
pub mod cache;
