//! Small shared helpers with no narrower home.
//!
//! Canonical implementations for cross-cutting one-liners that were
//! previously copy-pasted across `cognitive`, `analysis`, and friends.

use anyhow::Result;
use serde::Serialize;
use std::path::Path;

/// Current Unix epoch time in seconds.
///
/// Returns 0 if the system clock is set before the Unix epoch.
pub fn current_timestamp_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// Serialize `value` as pretty JSON and write it to `path`, creating parent
/// directories as needed.
pub fn save_json_pretty<T: Serialize + ?Sized>(value: &T, path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(value)?;
    std::fs::write(path, json)?;
    Ok(())
}
