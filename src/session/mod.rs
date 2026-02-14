//! Session and state management module
//!
//! This module contains session persistence and state management including:
//! - Checkpointing
//! - Time travel debugging
//! - Caching
//! - Local-first storage
//! - Edit history

pub mod checkpoint;
pub mod time_travel;
pub mod local_first;
pub mod edit_history;

#[cfg(feature = "cache")]
pub mod cache;
