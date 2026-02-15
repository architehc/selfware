//! Session and state management module
//!
//! This module contains session persistence and state management including:
//! - Checkpointing
//! - Time travel debugging
//! - Caching
//! - Local-first storage
//! - Edit history

pub mod checkpoint;
pub mod edit_history;
pub mod local_first;
pub mod time_travel;

#[cfg(feature = "cache")]
pub mod cache;
