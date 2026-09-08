//! Best-snapshot restore: submit the best state, not the last state.
//!
//! Opus 5 consult (2026-08-25, six-model panel on the TB 3.0 evidence): a
//! task that was 80% green at minute 30 submits a broken edit at minute 60.
//! The harness snapshots the agent's written/edited files whenever its own
//! verification passes, and restores that last-green state when the run fails
//! (abort, stall, budget stop). Read-only tasks and cancellations are never
//! touched.

use std::path::{Path, PathBuf};

/// File-backed snapshot of the agent's written deliverables, kept outside the
/// workspace (temp dir) so a broken end state can't corrupt it.
pub(crate) struct AgentSnapshot {
    dir: PathBuf,
    taken: bool,
}

impl Default for AgentSnapshot {
    fn default() -> Self {
        Self {
            dir: std::env::temp_dir().join(format!("selfware-snapshot-{}", std::process::id())),
            taken: false,
        }
    }
}

impl AgentSnapshot {
    fn slot_for(&self, path: &Path) -> PathBuf {
        // Flatten the path into a unique filename: /a/b/c.py -> _a_b_c.py
        let flat = path
            .to_string_lossy()
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_' {
                    c
                } else {
                    '_'
                }
            })
            .collect::<String>();
        self.dir.join(flat)
    }

    /// Copy each existing file into the snapshot (last-green capture).
    /// Missing files are skipped — a deleted file is not a snapshot target.
    pub(crate) fn snapshot_written(&mut self, paths: &[PathBuf]) -> std::io::Result<()> {
        std::fs::create_dir_all(&self.dir)?;
        for path in paths {
            if path.is_file() {
                std::fs::copy(path, self.slot_for(path))?;
                self.taken = true;
            }
        }
        Ok(())
    }

    /// True when at least one file has been snapshotted.
    pub(crate) fn has_snapshot(&self) -> bool {
        self.taken
    }

    /// Drop any snapshotted state (fresh task).
    pub(crate) fn clear(&mut self) {
        self.taken = false;
        if self.dir.exists() {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    /// Restore snapshotted files to their original paths. No-op for paths
    /// never snapshotted.
    pub(crate) fn restore_written(&self, paths: &[PathBuf]) -> std::io::Result<()> {
        for path in paths {
            let slot = self.slot_for(path);
            if slot.is_file() {
                if let Some(parent) = path.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                std::fs::copy(&slot, path)?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "../../tests/unit/agent/best_snapshot_test.rs"]
mod best_snapshot_test;
